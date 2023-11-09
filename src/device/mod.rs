use crate::{cache::Cache, map::Flags, store::Store};
use lazy_static::lazy_static;
use prometheus::{register_histogram, register_int_counter, Histogram, IntCounter};
use std::io;

lazy_static! {
    static ref IO_READ_BYTES: IntCounter =
        register_int_counter!("io_read_bytes", "number of bytes read").unwrap();
    static ref IO_WRITE_BYTES: IntCounter =
        register_int_counter!("io_write_bytes", "number of bytes written").unwrap();
    static ref IO_READ_OP: IntCounter =
        register_int_counter!("io_read_op", "number of read io operations").unwrap();
    static ref IO_READ_ERR: IntCounter =
        register_int_counter!("io_read_err", "number of read errors").unwrap();
    static ref IO_WRITE_OP: IntCounter =
        register_int_counter!("io_write_op", "number of write io operations").unwrap();
    static ref IO_WRITE_ERR: IntCounter =
        register_int_counter!("io_write_err", "number of write errors").unwrap();
    static ref BLOCKS_EVICTED: IntCounter =
        register_int_counter!("blocks_evicted", "number of blocks evicted").unwrap();
    static ref DEVICE_FLUSH: IntCounter =
        register_int_counter!("device_flush", "number of flush requests").unwrap();
    static ref IO_READ_HISTOGRAM: Histogram =
        register_histogram!("io_read_histogram", "read io histogram", vec![0.001, 0.05, 0.1, 0.5]).unwrap();
    static ref IO_WRITE_HISTOGRAM: Histogram =
        register_histogram!("io_write_histogram", "write io histogram", vec![0.001, 0.05, 0.1, 0.5]).unwrap();
    // TODO add histograms for both read/write and evict operations
}

const FLUSH_LENGTH: usize = 4;
/// Flush range is a tuple of location and length
/// of a range to be flushed
/// [start, end[
#[derive(Default, Clone, Copy)]
struct FlushRange(usize, usize);

impl FlushRange {
    #[inline]
    fn contains(&self, location: usize) -> bool {
        location >= self.0 && location < self.1
    }

    fn start(&self) -> usize {
        self.0
    }

    fn len(&self) -> usize {
        self.1 - self.0
    }

    fn append(&mut self, location: usize) -> Option<Self> {
        if self.contains(location) {
            return None;
        }

        // [-, -, -, -]

        // if this block is sequential to
        // the current range, append it if len won't be more the
        // allowed length
        if location == self.1 && self.len() < FLUSH_LENGTH {
            self.1 += 1;
            return None;
        }

        // otherwise create a new range and flush this one
        // and update self
        let f = *self;
        self.0 = location;
        self.1 = location + 1;

        if f.len() == 0 {
            None
        } else {
            Some(f)
        }
    }
}

/// implementation of the nbd device
///
/// The device mainly works against a cache object
/// which works as a layer on top of persisted storage
pub struct Device<S>
where
    S: Store,
{
    cache: Cache<S>,
    flush: FlushRange,
}

impl<S> Device<S>
where
    S: Store,
{
    pub fn new(cache: Cache<S>) -> Self {
        Self {
            cache,
            flush: FlushRange::default(),
        }
    }

    /// we can only map blocks index that fits in a u32.
    /// this is because
    pub fn block_of(&self, offset: u64) -> io::Result<u32> {
        let block = offset as usize / self.cache.block_size();

        u32::try_from(block).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
    }

    async fn inner_read(&mut self, offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        // find the block

        let mut index = self.block_of(offset)?;
        // TODO: make sure that index is not beyond the max index size by
        // the cold store.

        let mut inner_offset = offset as usize % self.cache.block_size();

        loop {
            let block = self.cache.get(index).await?;

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
        let mut inner_offset = offset as usize % self.cache.block_size();

        loop {
            let mut block = self.cache.get_mut(index).await?;
            let dest = &mut block.data_mut()[inner_offset..];
            let to_copy = std::cmp::min(dest.len(), buf.len());
            dest[..to_copy].copy_from_slice(&buf[..to_copy]);

            // mark it dirty because it was modified
            block.header_mut().set(Flags::Dirty, true);

            if let Some(flush) = self.flush.append(block.location()) {
                self.cache.flush_range(flush.start(), flush.len())?;
            }

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
impl<S> nbd_async::BlockDevice for Device<S>
where
    S: Store,
{
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let _timer = IO_READ_HISTOGRAM.start_timer();
        match self.inner_read(offset, buf).await {
            Ok(_) => {
                IO_READ_OP.inc();
                IO_READ_BYTES.inc_by(buf.len() as u64);

                Ok(())
            }
            Err(err) => {
                log::error!("read error {err:#}");
                IO_READ_ERR.inc();
                Err(err)
            }
        }
    }

    /// Write a block of data at offset.
    async fn write(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        let _timer = IO_WRITE_HISTOGRAM.start_timer();
        match self.inner_write(offset, buf).await {
            Ok(_) => {
                IO_WRITE_OP.inc();
                IO_WRITE_BYTES.inc_by(buf.len() as u64);
                Ok(())
            }
            Err(err) => {
                log::error!("write error {err:#}");
                IO_WRITE_ERR.inc();
                Err(err)
            }
        }
    }

    /// Flushes write buffers to the underlying storage medium
    async fn flush(&mut self) -> io::Result<()> {
        DEVICE_FLUSH.inc();
        self.cache.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::{Cache, NullStore};
    use bytesize::ByteSize;
    use nbd_async::BlockDevice;

    #[tokio::test]
    async fn read() {
        const PATH: &str = "/tmp/device.read.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

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

        let cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

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

    #[test]
    fn flush_range() {
        let mut range = FlushRange::default();
        assert!(range.append(1).is_none());
        assert!(range.append(1).is_none());
        assert!(range.append(1).is_none());
        assert!(range.append(2).is_none());
        assert!(range.append(3).is_none());

        let flush = range.append(5);
        assert!(flush.is_some());
        let flush = flush.unwrap();
        assert_eq!(flush.start(), 1);
        assert_eq!(flush.len(), 3);

        assert_eq!(range.start(), 5);
        assert_eq!(range.len(), 1);

        assert!(range.append(6).is_none());
        assert!(range.append(7).is_none());
        assert!(range.append(8).is_none());
        // this one will make it flush because there are more than 4 blocks
        // in the range
        let flush = range.append(9);
        assert!(flush.is_some());

        let flush = flush.unwrap();
        assert_eq!(flush.start(), 5);
        assert_eq!(flush.len(), 4);
        assert_eq!(range.start(), 9);
        assert_eq!(range.len(), 1);
    }
}
