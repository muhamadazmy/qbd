use memmap2::MmapMut;
use std::io;
use tokio::fs;

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

#[tokio::main]
async fn main() {
    // create a 10MB block device

    let file = fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .open("/tmp/disk.nbd")
        .await
        .unwrap();

    file.set_len(10 * 1024 * 1024).await.unwrap();

    let map = unsafe { memmap2::MmapOptions::new().map_mut(&file).unwrap() };
    let block = MapDev { map };
    nbd_async::serve_local_nbd("/dev/nbd0", 1024, 10 * 1024, false, block)
        .await
        .unwrap();
}
