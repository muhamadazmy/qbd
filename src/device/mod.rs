use crate::{
    cache::{Cache, Sink},
    map::{Block, Flags, Header},
    store::{self, Store},
    Result,
};
use lazy_static::lazy_static;
use prometheus::{register_int_counter, IntCounter};
use std::io;
use std::sync::Arc;
use tokio::sync::RwLock;

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

const FLUSH_LENGTH: usize = 4;

struct StoreSink<S>
where
    S: Store,
{
    store: Arc<RwLock<S>>,
}

impl<S> StoreSink<S>
where
    S: Store,
{
    fn new(store: Arc<RwLock<S>>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl<S> Sink for StoreSink<S>
where
    S: Store,
{
    async fn evict(&mut self, index: u32, block: Block<'_>) -> Result<()> {
        if !block.header().flag(Flags::Dirty) {
            log::debug!("evict: {index} .. skipped");
            return Ok(());
        }
        log::debug!("evict: {index}");
        BLOCKS_EVICTED.inc();

        let mut store = self.store.write().await;
        store
            .set(index, block.data())
            .await
            .map_err(io::Error::from)?;

        Ok(())
    }
}

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

pub struct Device<S>
where
    S: Store,
{
    cache: Cache,
    evict: StoreSink<S>,
    flush: FlushRange,
    store: Arc<RwLock<S>>,
}

impl<S> Device<S>
where
    S: Store,
{
    pub fn new(cache: Cache, store: S) -> Self {
        let store = Arc::new(RwLock::new(store));
        Self {
            cache,
            evict: StoreSink::new(Arc::clone(&store)),
            flush: FlushRange::default(),
            store,
        }
    }

    /// we can only map blocks index that fits in a u32.
    /// this is because
    pub fn block_of(&self, offset: u64) -> io::Result<u32> {
        let block = offset as usize / self.cache.block_size();

        u32::try_from(block).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
    }

    // async fn get_or_load(&mut self, index: u32) -> io::Result<BlockMut> {
    //     let mut slot = self
    //         .cache
    //         .put(Header::new(index), None, &mut self.evict)
    //         .await?;

    //     // this is done here not by passing data directly to put
    //     // because evict might also try to hold the store lock
    //     // which will cause a deadlock
    //     // so instead we get the block as is from cache and fill it
    //     // up
    //     let store = self.store.read().await;
    //     let data = store.get(index).await?;
    //     if let Some(data) = data {
    //         slot.data_mut().copy_from_slice(&data);
    //         slot.update_crc()
    //     }

    //     Ok(slot)
    // }

    async fn inner_read(&mut self, offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        // find the block

        let mut index = self.block_of(offset)?;
        // TODO: make sure that index is not beyond the max index size by
        // the cold store.

        let mut inner_offset = offset as usize % self.cache.block_size();

        loop {
            let block = self.cache.get(index);
            // TODO: instead of initializing an empty block we should fetch from
            // cold storage. this is temporary
            let block = match block {
                Some(block) => block,
                None => {
                    let mut slot = self
                        .cache
                        .put(Header::new(index), None, &mut self.evict)
                        .await?;

                    // this is done here not by passing data directly to put
                    // because evict might also try to hold the store lock
                    // which will cause a deadlock
                    // so instead we get the block as is from cache and fill it
                    // up
                    let store = self.store.read().await;
                    let data = store.get(index).await?;
                    if let Some(data) = data {
                        log::debug!("load: {index}");
                        slot.data_mut().copy_from_slice(&data);
                        slot.update_crc()
                    }

                    slot.into()
                }
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
        let mut inner_offset = offset as usize % self.cache.block_size();

        loop {
            let block = self.cache.get_mut(index);
            // TODO: instead of initializing an empty block we should fetch from
            // cold storage. this is temporary
            let mut block = match block {
                Some(block) => block,
                None => {
                    let mut slot = self
                        .cache
                        .put(Header::new(index), None, &mut self.evict)
                        .await?;

                    // this is done here not by passing data directly to put
                    // because evict might also try to hold the store lock
                    // which will cause a deadlock
                    // so instead we get the block as is from cache and fill it
                    // up
                    let store = self.store.read().await;
                    let data = store.get(index).await?;
                    if let Some(data) = data {
                        log::debug!("load: {index}");
                        slot.data_mut().copy_from_slice(&data);
                        slot.update_crc()
                    }

                    slot
                }
            };

            let dest = &mut block.data_mut()[inner_offset..];
            let to_copy = std::cmp::min(dest.len(), buf.len());
            dest[..to_copy].copy_from_slice(&buf[..to_copy]);

            block.set_header(block.header().with_flag(Flags::Dirty, true));

            if let Some(flush) = self.flush.append(block.location()) {
                block.map().flush_blocks(flush.start(), flush.len())?;
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

pub struct NoStore;
#[async_trait::async_trait]
impl Store for NoStore {
    async fn set(&mut self, _index: u32, _block: &[u8]) -> store::Result<()> {
        unimplemented!()
    }
    async fn get(&self, _index: u32) -> store::Result<Option<store::Data>> {
        unimplemented!()
    }
    fn size(&self) -> usize {
        unimplemented!()
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

        let mut dev = Device::new(cache, NoStore);

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

        let mut dev = Device::new(cache, NoStore);

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
