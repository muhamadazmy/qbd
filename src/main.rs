use anyhow::Context;
use bytesize::ByteSize;
use clap::{ArgAction, Parser};
use nbd_async::Control;
use qbd::{
    device::DeviceControl,
    store::{ConcatStore, FileStore, Store},
    *,
};
use std::{
    fmt::Display, future, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc, time::Duration,
};
use tokio::sync::mpsc::{channel, Sender};
use tokio_stream::wrappers::ReceiverStream;

/// Send an evict control signal to the device every 500 milliseconds
/// the device can choose to ignore that
const EVICT_DURATION: Duration = Duration::from_millis(500);

/// This wrapper is only to overcome the default
/// stupid format of ByteSize which uses MB/GB units instead
/// of MiB/GiB units
#[derive(Debug, Clone, PartialEq, Eq)]
struct BSWrapper(ByteSize);

impl FromStr for BSWrapper {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let size = ByteSize::from_str(s)?;
        Ok(Self(size))
    }
}

impl Display for BSWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.to_string_as(true).as_str())
    }
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(name="qbd", author, version = env!("GIT_VERSION"), about, long_about = None)]
struct Args {
    /// path to nbd device to attach to
    #[arg(short, long)]
    nbd: PathBuf,

    /// path to the cache file, usually should reside on SSD storage
    #[arg(short, long)]
    cache: PathBuf,

    /// cache size has to be multiple of page-size
    #[arg(long, default_value_t=BSWrapper(bytesize::ByteSize::gib(10)))]
    cache_size: BSWrapper,

    /// page size used for both cache and storage
    #[arg(long, default_value_t=BSWrapper(bytesize::ByteSize::mib(1)))]
    page_size: BSWrapper,

    /// url to backend store as `file:///path/to/file?size=SIZE`
    /// accepts multiple stores, the total size of the disk
    /// is the total size of all stores provided
    #[arg(long, required = true)]
    store: Vec<url::Url>,

    /// listen address for metrics. metrics will be available at /metrics
    #[arg(short, long, default_value_t = SocketAddr::from(([127, 0, 0, 1], 9000)))]
    metrics: SocketAddr,

    /// disable metrics server
    #[arg(long)]
    disable_metrics: bool,

    /// enable debugging logs
    #[clap(short, long, action=ArgAction::Count)]
    debug: u8,
}

async fn app(args: Args) -> anyhow::Result<()> {
    let cache_size = args.cache_size.0;
    let page_size = args.page_size.0;

    if cache_size.as_u64() % page_size.as_u64() != 0 {
        anyhow::bail!("cache-size must be multiple of page-size");
    }

    // todo: probably move building of a store from url
    // somewhere else
    let mut stores = vec![];
    for u in &args.store {
        if u.scheme() != "file" {
            anyhow::bail!("only store type `file` is supported");
        }

        let size = u.query_pairs().find(|(key, _)| key == "size");
        let size = match size {
            Some((_, size)) => ByteSize::from_str(&size)
                .map_err(|e| anyhow::anyhow!("failed to parse store size: {e}"))?,
            None => anyhow::bail!("size param is required in store url"),
        };

        stores.push(
            FileStore::new(u.path(), size, page_size)
                .with_context(|| format!("failed to create store {u}"))?,
        );
    }

    let store = ConcatStore::new(stores)?;

    let disk_size = store.size();
    log::info!(
        "size: {} cache-size: {}, page-size: {}",
        disk_size.to_string_as(true),
        cache_size.to_string_as(true),
        page_size.to_string_as(true)
    );

    let cache = cache::Cache::new(store, args.cache, cache_size, page_size)
        .context("failed to create cache")?;

    let device = device::Device::new(cache);

    let registry = Arc::new(prometheus::default_registry().clone());

    if !args.disable_metrics {
        tokio::spawn(prometheus_hyper::Server::run(
            registry,
            args.metrics,
            future::pending(),
        ));
    }

    let (ctl, recv) = channel(1);

    handle_signals(ctl.clone()).context("handling hangup signals")?;

    tokio::spawn(async move {
        // this keep sending control jobs to the device.
        // we attach a device control object carries a command (evict).
        // the device will only handle this is if it has been
        // ideal for that evict_duration
        let msg = DeviceControl::evict(EVICT_DURATION);
        loop {
            if ctl.send(Control::Notify(msg)).await.is_err() {
                break;
            }
            tokio::time::sleep(EVICT_DURATION).await;
        }
    });

    let nbd_bs = ByteSize::kib(4);
    nbd_async::serve_local_nbd(
        args.nbd,
        nbd_bs.0 as u32,
        disk_size.0 / nbd_bs.0,
        false,
        device,
        ReceiverStream::new(recv),
    )
    .await?;

    log::info!("shutting down");
    Ok(())
}

fn handle_signals(ctr: Sender<Control<DeviceControl>>) -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};

    // it's stupid we can't have one channel for all signals
    let mut hu = signal(SignalKind::hangup())?;
    let mut iu = signal(SignalKind::interrupt())?;
    let mut te = signal(SignalKind::terminate())?;

    tokio::spawn(async move {
        tokio::select! {
            _ = hu.recv() => {},
            _ = iu.recv() => {},
            _ = te.recv() => {},
        }

        let _ = ctr.send(Control::Shutdown).await;
    });

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // create a 10MB block device
    let args = Args::parse();

    simple_logger::SimpleLogger::new()
        .with_utc_timestamps()
        .with_level({
            match args.debug {
                0 => log::LevelFilter::Info,
                1 => log::LevelFilter::Debug,
                _ => log::LevelFilter::Trace,
            }
        })
        .init()?;

    if let Err(err) = app(args).await {
        eprintln!("error while initializing device: {:#}", err);
        std::process::exit(1);
    }

    Ok(())
}
