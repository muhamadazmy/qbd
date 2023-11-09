//! the cache module implements caching on top of blocks map
//! the idea is that this in memory cache can track which block
//! is stored at what index
//!
//! the block index used in get/put operations are the full range
//! of blocks supported by the nbd device. If using block size of
//! 1MiB this maps to 4096TiB
//!
use std::{num::NonZeroUsize, path::Path};

use crate::{
    map::{Block, BlockMut, Flags},
    store::{Data, Store},
};

use super::map::BlockMap;
use bytesize::ByteSize;
use lazy_static::lazy_static;
use lru::LruCache;
use prometheus::{
    register_histogram, register_int_counter, register_int_gauge, Histogram, IntCounter, IntGauge,
};

use crate::{Error, Result};

lazy_static! {
    static ref BLOCKS_EVICTED: IntCounter =
        register_int_counter!("blocks_evicted", "number of blocks evicted from backend").unwrap();
    static ref BLOCKS_LOADED: IntCounter =
        register_int_counter!("blocks_loaded", "number of blocks loaded from backend").unwrap();
    static ref BLOCKS_CACHED: IntGauge =
        register_int_gauge!("blocks_cached", "number of blocks available in cache").unwrap();
    static ref EVICT_HISTOGRAM: Histogram = register_histogram!(
        "evict_histogram",
        "evict histogram",
        vec![0.001, 0.05, 0.1, 0.5]
    )
    .unwrap();
    static ref LOAD_HISTOGRAM: Histogram = register_histogram!(
        "load_histogram",
        "load histogram",
        vec![0.001, 0.05, 0.1, 0.5]
    )
    .unwrap();
}

/// CachedBlock holds information about blocks in lru memory
struct CachedBlock {
    /// location of the block in underlying cache
    location: usize,
    // in memory information
    // about the block can be here
}

/// Cache layer on top of BlockMap. This allows tracking what block is in what map location
/// and make it easier to find which block in the map is least used so we can evict if needed
pub struct Cache<S>
where
    S: Store,
{
    cache: LruCache<u32, CachedBlock>,
    map: BlockMap,
    store: S,
    // blocks is number of possible blocks
    // in the store (store.size() / bs)
    blocks: usize,
}

impl<S> Cache<S>
where
    S: Store,
{
    pub fn new<P: AsRef<Path>>(store: S, path: P, size: ByteSize, bs: ByteSize) -> Result<Self> {
        let map = BlockMap::new(path, size, bs)?;
        let bc = size.as_u64() / bs.as_u64();

        let mut cache = LruCache::new(NonZeroUsize::new(bc as usize).ok_or(Error::ZeroSize)?);

        for block in map.iter() {
            let header = block.header();
            if header.flag(Flags::Occupied) {
                cache.put(
                    header.block(),
                    CachedBlock {
                        location: block.location(),
                    },
                );
            }
        }

        BLOCKS_CACHED.set(cache.len() as i64);
        // to be able to check block boundaries
        let blocks = store.size().as_u64() / bs.as_u64();
        log::debug!("device blocks: {blocks}");
        Ok(Self {
            map,
            cache,
            store,
            blocks: blocks as usize,
        })
    }

    pub fn inner(self) -> S {
        self.store
    }

    pub fn block_size(&self) -> usize {
        self.map.block_size()
    }

    pub fn block_count(&self) -> usize {
        self.map.block_count()
    }

    pub fn occupied(&self) -> usize {
        self.map
            .iter()
            .filter(|b| b.header().flag(Flags::Occupied))
            .count()
    }
    /// gets the block with id <block> if already in cache, other wise return None
    /// TODO: enhance access to this method. the `mut` is only needed to allow
    /// the lru cache to update, but the block itself doesn't need it because it
    /// requires no mut borrowing. But then multiple calls to get won't be possible
    /// because i will need exclusive access to this, which will slow down read
    /// access.
    pub async fn get(&mut self, block: u32) -> Result<Block> {
        // we first hit the mem cache see if there is a block tracked here
        if block as usize >= self.blocks {
            return Err(Error::BlockIndexOutOfRange);
        }
        let item = self.cache.get(&block);
        match item {
            Some(cached) => Ok(self.map.at(cached.location)),
            None => self.warm(block).await.map(Block::from),
        }
    }

    /// get a BlockMut
    pub async fn get_mut(&mut self, block: u32) -> Result<BlockMut> {
        if block as usize >= self.blocks {
            return Err(Error::BlockIndexOutOfRange);
        }

        let item = self.cache.get(&block);
        match item {
            Some(cached) => Ok(self.map.at_mut(cached.location)),
            None => self.warm(block).await,
        }
    }

    async fn warm(&mut self, block: u32) -> Result<BlockMut> {
        // first find which block to evict.

        let mut blk: BlockMut;
        if self.cache.len() < self.cache.cap().get() {
            // the map still has free slots then
            blk = self.map.at_mut(self.cache.len());
        } else {
            // other wise, we need to evict one of the blocks from the map file
            // so wee peek into lru find out which one we can kick out first.

            // we know that the cache is full, so this will always return Some
            let (block_index, item) = self.cache.peek_lru().unwrap();
            // so block block_index stored at map location item.location
            // can be evicted
            blk = self.map.at_mut(item.location);

            // store this in permanent store
            // eviction should only happen if blk is dirty
            // note it's up to user of the cache to mark blocks as
            // dirty otherwise they won't evict to backend
            if blk.header().flag(Flags::Dirty) {
                log::debug!("block {} eviction", *block_index);
                BLOCKS_EVICTED.inc();
                let timer = EVICT_HISTOGRAM.start_timer();
                self.store.set(*block_index, blk.data()).await?;
                timer.observe_duration();
            } else {
                log::trace!("block {} eviction skipped", *block_index);
            }

            // now the block location is ready to be reuse
            // note that the next call to push will actually remove that item from the lru
        }

        blk.header_mut()
            .set_block(block)
            .set(Flags::Dirty, false)
            .set(Flags::Occupied, true);

        assert_eq!(blk.header().block(), block, "block header update");
        let timer = LOAD_HISTOGRAM.start_timer();
        let data = self.store.get(block).await?;
        timer.observe_duration();
        if let Some(data) = data {
            // override block
            BLOCKS_LOADED.inc();
            log::trace!("warming cache for block {block}");
            blk.data_mut().copy_from_slice(&data);
            blk.update_crc();
        } else {
            // should we zero it out ?
            // or not
        }

        self.cache.push(
            block,
            CachedBlock {
                location: blk.location(),
            },
        );

        BLOCKS_CACHED.set(self.cache.len() as i64);
        Ok(blk)
    }

    pub fn flush(&self) -> Result<()> {
        self.map.flush_async()?;
        Ok(())
    }

    pub fn flush_range(&self, location: usize, count: usize) -> Result<()> {
        self.map.flush_range_async(location, count)
    }
}

pub struct NullStore;

#[async_trait::async_trait]
impl Store for NullStore {
    type Vec = Vec<u8>;

    async fn set(&mut self, _index: u32, _block: &[u8]) -> Result<()> {
        Ok(())
    }
    async fn get(&self, _index: u32) -> Result<Option<Data<Self::Vec>>> {
        Ok(None)
    }
    fn size(&self) -> ByteSize {
        ByteSize::b(u64::MAX)
    }

    fn block_size(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod test {
    use crate::store;

    use super::*;

    #[tokio::test]
    async fn test_cache_new() {
        const PATH: &str = "/tmp/cache.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let mut cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        let block = cache.get_mut(20).await;
        //this block does not exist in the cache file yet.
        // the NullStore is HUGE in size, so while the cache size is only 10kib
        // blocks can be retrieved behind the cache size but as long as they
        // are
        assert!(block.is_ok());

        let mut block = block.unwrap();
        assert!(!block.header().flag(Flags::Dirty));
        assert!(block.header().flag(Flags::Occupied));
        assert!(block.data().iter().all(|f| *f == 0));
        assert_eq!(block.data().len(), 1024);

        block.data_mut().fill(10);
        block.header_mut().set(Flags::Dirty, true);

        let block = cache.get(20).await;
        assert!(block.is_ok());

        let block = block.unwrap();

        assert!(block.header().flag(Flags::Dirty));
        assert!(block.header().flag(Flags::Occupied));
        assert!(block.data().iter().all(|f| *f == 10));
        assert_eq!(block.data().len(), 1024);
    }

    #[tokio::test]
    async fn test_cache_reload() {
        const PATH: &str = "/tmp/cache.reload.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let mut cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        let block = cache.get_mut(20).await;
        //this block does not exist in the cache file yet.
        // the NullStore is HUGE in size, so while the cache size is only 10kib
        // blocks can be retrieved behind the cache size but as long as they
        // are
        assert!(block.is_ok());

        let mut block = block.unwrap();
        assert!(!block.header().flag(Flags::Dirty));
        assert!(block.header().flag(Flags::Occupied));
        assert!(block.data().iter().all(|f| *f == 0));
        assert_eq!(block.data().len(), 1024);

        block.data_mut().fill(10);
        block.header_mut().set(Flags::Dirty, true);

        // drop cache
        drop(cache);

        let mut cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        // block 0 was not here before just to make sure
        let block = cache.get(0).await;
        assert!(block.is_ok());

        let block = block.unwrap();
        assert!(!block.header().flag(Flags::Dirty));
        assert!(block.header().flag(Flags::Occupied));
        assert!(block.data().iter().all(|f| *f == 0));
        assert_eq!(block.data().len(), 1024);

        // this is from before the drop it should still be fine
        let block = cache.get(20).await;
        assert!(block.is_ok());

        let block = block.unwrap();

        assert!(block.header().flag(Flags::Dirty));
        assert!(block.header().flag(Flags::Occupied));
        assert!(block.data().iter().all(|f| *f == 10));
        assert_eq!(block.data().len(), 1024);
    }

    #[tokio::test]
    async fn test_eviction() {
        const PATH: &str = "/tmp/cache.eviction.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        // store of 10k
        let mem = store::InMemory::new(10);

        assert_eq!(mem.size(), ByteSize::kib(10));
        // cache of 5k and bs of 1k
        let mut cache = Cache::new(mem, PATH, ByteSize::kib(5), ByteSize::kib(1)).unwrap();

        assert_eq!(cache.block_count(), 5);

        let block = cache.get_mut(9).await;
        assert!(block.is_ok());
        let mut block = block.unwrap();
        assert_eq!(block.location(), 0); // sanity check
                                         // we need this otherwise cache won't evict it
        block.header_mut().set(Flags::Dirty, true);
        // fill it with something
        block.data_mut().fill_with(|| 7);
        block.update_crc();

        assert_eq!(cache.occupied(), 1);

        // cache can hold only 5 blocks. It already now holds 1 (block 9). If we get 5 more, block 9 should be evicted
        assert_eq!(cache.get(0).await.unwrap().location(), 1);
        assert_eq!(cache.get(1).await.unwrap().location(), 2);
        assert_eq!(cache.get(2).await.unwrap().location(), 3);
        assert_eq!(cache.get(3).await.unwrap().location(), 4);
        assert_eq!(cache.get(4).await.unwrap().location(), 0);
        assert_eq!(cache.get(5).await.unwrap().location(), 1);

        assert_eq!(cache.occupied(), 5);

        let mem = cache.inner();

        // while we should except 2 blocks more evicted because we
        // have pushed total of 7 blocks, but only block 9 was dirty
        // hence block 0 (the last to be evicted) is in fact not dirty
        assert_eq!(mem.mem.len(), 1);

        assert!(mem.mem.get(&9).is_some());

        // open cache again with the same memory
        let mut cache = Cache::new(mem, PATH, ByteSize::kib(5), ByteSize::kib(1)).unwrap();

        let block = cache.get(9).await;
        assert!(block.is_ok());
        let block = block.unwrap();
        // sanity check
        assert_eq!(block.location(), 0);
        // the block here was retrieved from map, so it shouldn't be dirty
        assert!(!block.header().flag(Flags::Dirty));
        assert!(block.data().iter().all(|v| *v == 7));
        assert!(block.is_crc_ok());

        assert_eq!(cache.occupied(), 5);
    }
}
