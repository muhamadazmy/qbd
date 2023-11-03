use bytesize::ByteSize;
use clap::{ArgAction, Parser};
use qbd::*;
use std::{fmt::Display, path::PathBuf, str::FromStr};

/// This wrapper is only to overcome the default
/// stupid format of ByteSize which uses MB/GB units instead
/// of MiB/GiB units
#[derive(Debug, Clone, PartialEq, Eq)]
struct BSWrapper(ByteSize);

impl FromStr for BSWrapper {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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

    // TODO display of
    /// cache size has to be multiple of block-size
    #[arg(long, default_value_t=BSWrapper(bytesize::ByteSize::gib(10)))]
    cache_size: BSWrapper,

    #[arg(long, default_value_t=BSWrapper(bytesize::ByteSize::mib(1)))]
    block_size: BSWrapper,

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

    let cache = cache::Cache::new(args.cache, cache_size, block_size)?;

    let device = device::Device::new(cache);

    nbd_async::serve_local_nbd(args.nbd, 1024, cache_size.as_u64() / 1024, false, device).await?;

    Ok(())
}
