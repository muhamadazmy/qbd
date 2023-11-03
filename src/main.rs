use clap::{ArgAction, Parser};
use memmap2::MmapMut;
use std::{io, num::NonZeroU16, path::PathBuf};
use tokio::fs;

mod cache;
mod map;

struct MapDev {
    map: MmapMut,
}

#[async_trait::async_trait(?Send)]
impl nbd_async::BlockDevice for MapDev {
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let offset = offset as usize;
        buf.copy_from_slice(&self.map[offset..offset + buf.len()]);
        Ok(())
    }
    /// Write a block of data at offset.
    async fn write(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        let offset = offset as usize;
        self.map[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(())
    }
    /// Flushes write buffers to the underlying storage medium
    async fn flush(&mut self) -> io::Result<()> {
        self.map.flush_async()
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
    #[arg(long, default_value_t=bytesize::ByteSize::gb(10))]
    cache_size: bytesize::ByteSize,

    #[arg(long, default_value_t=bytesize::ByteSize::mb(1))]
    block_size: bytesize::ByteSize,

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

    if args.cache_size.as_u64() % args.block_size.as_u64() != 0 {
        anyhow::bail!("cache-size must be multiple of block-size");
    }

    let cache = cache::Cache::new(args.cache, args.cache_size, args.block_size)?;
    //let cache = cache::Cache::new(args.cache, bc, bs)
    let file = fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .open("/tmp/disk.nbd")
        .await?;

    file.set_len(10 * 1024 * 1024).await.unwrap();

    let map = unsafe { memmap2::MmapOptions::new().map_mut(&file).unwrap() };
    let block = MapDev { map };
    nbd_async::serve_local_nbd("/dev/nbd0", 1024, 10 * 1024, false, block).await?;

    Ok(())
}
