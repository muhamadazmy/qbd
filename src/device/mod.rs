use crate::{
    cache::{self, Cache, EvictSink},
    map::{Block, Flags, Header},
};
use lazy_static::lazy_static;
use prometheus::{register_int_counter, IntCounter};
use std::io;

lazy_static! {
    static ref IO_READ_BYTES: IntCounter =
        register_int_counter!("io_read_bytes", "number of bytes read").unwrap();
    static ref IO_WRITE_BYTES: IntCounter =
        register_int_counter!("io_wite_bytes", "number of bytes written").unwrap();
    static ref IO_READ_OP: IntCounter =
        register_int_counter!("io_read_op", "number of read io operations").unwrap();
    static ref IO_READ_ERR: IntCounter =
        register_int_counter!("io_read_err", "number of read errors").unwrap();
    static ref IO_WRITE_OP: IntCounter =
        register_int_counter!("io_wite_op", "number of write io operations").unwrap();
    static ref IO_WRITE_ERR: IntCounter =
        register_int_counter!("io_wite_err", "number of write errors").unwrap();
    static ref BLOCKS_EVICTED: IntCounter =
        register_int_counter!("blocks_evicted", "number of blocks evicted").unwrap();

    // TODO add histograms for both read/write and evict operations
}

struct StoreSink;

#[async_trait::async_trait]
impl EvictSink for StoreSink {
    async fn evict(&mut self, _index: u32, _block: Block<'_>) -> cache::Result<()> {
        BLOCKS_EVICTED.inc();

        Ok(())
    }
}

pub struct Device {
    cache: Cache,
    evict: StoreSink,
}

impl Device {
    pub fn new(cache: Cache) -> Self {
        Self {
            cache,
            evict: StoreSink,
        }
    }

    /// we can only map blocks index that fits in a u32.
    /// this is because
    pub fn block_of(&self, offset: u64) -> io::Result<u32> {
        let block = offset / self.cache.block_size();

        u32::try_from(block).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
    }

    async fn inner_read(&mut self, offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        // find the block

        let mut index = self.block_of(offset)?;
        // TODO: make sure that index is not beyond the max index size by
        // the cold store.

        let mut inner_offset = (offset % self.cache.block_size()) as usize;

        loop {
            let block = self.cache.get(index);
            // TODO: instead of initializing an empty block we should fetch from
            // cold storage. this is temporary
            let block = match block {
                Some(block) => block,
                None => self
                    .cache
                    .put(Header::new(index), None, &mut self.evict)
                    .await?
                    .into(),
            };

            let source = &block.data()[inner_offset..];
            let to_copy = std::cmp::min(source.len(), buf.len());
            buf[..to_copy].copy_from_slice(&source[..to_copy]);
            buf = &mut buf[to_copy..];
            if buf.is_empty() {
                break;
            }
            index += 1;
            inner_offset = 0;
        }
        Ok(())
    }

    /// Write a block of data at offset.
    async fn inner_write(&mut self, offset: u64, mut buf: &[u8]) -> io::Result<()> {
        let mut index = self.block_of(offset)?;
        let mut inner_offset = (offset % self.cache.block_size()) as usize;

        loop {
            let block = self.cache.get_mut(index);
            // TODO: instead of initializing an empty block we should fetch from
            // cold storage. this is temporary
            let mut block = match block {
                Some(block) => block,
                None => {
                    self.cache
                        .put(Header::new(index), None, &mut self.evict)
                        .await?
                }
            };

            let dest = &mut block.data_mut()[inner_offset..];
            let to_copy = std::cmp::min(dest.len(), buf.len());
            dest[..to_copy].copy_from_slice(&buf[..to_copy]);

            block.set_header(block.header().with_flag(Flags::Dirty, true));

            // todo: this is a very heavy call that affects
            // the max speed of the write operation
            // may be we can schedule multiple of these later?
            // should we use the async form ?
            block.flush()?;

            buf = &buf[to_copy..];
            if buf.is_empty() {
                break;
            }
            index += 1;
            inner_offset = 0;
        }

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl nbd_async::BlockDevice for Device {
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        match self.inner_read(offset, buf).await {
            Ok(_) => {
                IO_READ_OP.inc();
                IO_READ_BYTES.inc_by(buf.len() as u64);

                Ok(())
            }
            Err(err) => {
                IO_READ_ERR.inc();
                Err(err)
            }
        }
    }

    /// Write a block of data at offset.
    async fn write(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        match self.inner_write(offset, buf).await {
            Ok(_) => {
                IO_WRITE_OP.inc();
                IO_WRITE_BYTES.inc_by(buf.len() as u64);
                Ok(())
            }
            Err(err) => {
                IO_WRITE_ERR.inc();
                Err(err)
            }
        }
    }

    /// Flushes write buffers to the underlying storage medium
    async fn flush(&mut self) -> io::Result<()> {
        self.cache.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::Cache;
    use bytesize::ByteSize;
    use nbd_async::BlockDevice;

    #[tokio::test]
    async fn read() {
        const PATH: &str = "/tmp/device.read.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let cache = Cache::new(PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        let mut dev = Device::new(cache);

        let mut buf: [u8; 512] = [1; 512];

        let result = dev.read(0, &mut buf).await;
        assert!(result.is_ok());
        assert!(!buf.contains(&1));

        buf.fill(1);
        let result = dev.read(512, &mut buf).await;
        assert!(result.is_ok());
        assert!(!buf.contains(&1));

        buf.fill(1);
        let result = dev.read(800, &mut buf).await;
        assert!(result.is_ok());
        assert!(!buf.contains(&1));

        buf.fill(1);
        let result = dev.read(1024, &mut buf).await;
        assert!(result.is_ok());
        assert!(!buf.contains(&1));
    }

    #[tokio::test]
    async fn write() {
        const PATH: &str = "/tmp/device.write.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let cache = Cache::new(PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        let mut dev = Device::new(cache);

        let mut buf: [u8; 512] = [1; 512];

        let result = dev.write(0, &buf).await;
        assert!(result.is_ok());

        buf.fill(2);
        let result = dev.write(512, &buf).await;
        assert!(result.is_ok());

        let result = dev.read(0, &mut buf).await;
        assert!(result.is_ok());
        assert!(buf.iter().all(|v| *v == 1));

        let result = dev.read(512, &mut buf).await;
        assert!(result.is_ok());
        assert!(buf.iter().all(|v| *v == 2));

        buf.fill(3);
        let result = dev.write(1024, &buf).await;
        assert!(result.is_ok());

        let mut buf: [u8; 1024] = [0; 1024];

        // we filled 512 bytes at offset 512 with 2
        // we also filled 512 at offset 1024 with 3
        // now we reading 1024 bytes from offset 512
        let result = dev.read(512, &mut buf).await;
        assert!(result.is_ok());
        assert!(buf[..512].iter().all(|v| *v == 2));
        assert!(buf[512..1024].iter().all(|v| *v == 3));
    }
}
