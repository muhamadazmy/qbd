use anyhow::Context;
use bytesize::ByteSize;
use clap::{ArgAction, Parser};
use qbd::{
    store::{ConcatStore, FileStore, Store},
    *,
};
use std::{fmt::Display, future, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};
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

    /// path to the cache file, usually should be SSD storage
    #[arg(short, long)]
    cache: PathBuf,

    /// cache size has to be multiple of block-size
    #[arg(long, default_value_t=BSWrapper(bytesize::ByteSize::gib(10)))]
    cache_size: BSWrapper,

    #[arg(long, default_value_t=BSWrapper(bytesize::ByteSize::mib(1)))]
    block_size: BSWrapper,

    /// url to backend store as `file:///path/to/file?size=SIZE`
    /// accepts multiple stores, the total size of the disk
    /// is the total size of all stores provided
    #[arg(long, required = true)]
    store: Vec<url::Url>,

    /// listen address for metrics. metrics will be available at /metrics
    #[arg(short, long, default_value_t = SocketAddr::from(([127, 0, 0, 1], 9000)))]
    metrics: SocketAddr,

    /// enable debugging logs
    #[clap(long, action=ArgAction::Count)]
    debug: u8,
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

    let cache_size = args.cache_size.0;
    let block_size = args.block_size.0;

    if cache_size.as_u64() % block_size.as_u64() != 0 {
        anyhow::bail!("cache-size must be multiple of block-size");
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
            FileStore::new(u.path(), size, block_size)
                .with_context(|| format!("failed to create store {u}"))?,
        );
    }

    let store = ConcatStore::new(stores)?;

    let disk_size = store.size();
    log::info!(
        "size: {} cache-size: {}, block-size: {}",
        disk_size.to_string_as(true),
        cache_size.to_string_as(true),
        block_size.to_string_as(true)
    );

    let cache = cache::Cache::new(store, args.cache, cache_size, block_size)?;

    let device = device::Device::new(cache);

    let registry = Arc::new(prometheus::default_registry().clone());
    tokio::spawn(prometheus_hyper::Server::run(
        registry,
        args.metrics,
        future::pending(),
    ));

    nbd_async::serve_local_nbd(args.nbd, 1024, disk_size.as_u64() / 1024, false, device).await?;

    Ok(())
}
