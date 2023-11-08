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
use prometheus::{register_int_counter, IntCounter};

use crate::{Error, Result};

lazy_static! {
    static ref BLOCKS_EVICTED: IntCounter =
        register_int_counter!("blocks_evicted", "number of blocks evicted from backend").unwrap();
    static ref BLOCKS_LOADED: IntCounter =
        register_int_counter!("blocks_loaded", "number of blocks loaded from backend").unwrap();

    // TODO add histograms for both read/write and evict operations
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

        // to be able to check block boundaries
        let blocks = store.size().as_u64() / bs.as_u64();
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
                self.store.set(*block_index, blk.data()).await?;
            } else {
                log::debug!("block {} eviction skipped", *block_index);
            }

            // now the block location is ready to be reuse
            // note that the next call to push will actually remove that item from the lru
        }

        blk.header_mut()
            .set_block(block)
            .set(Flags::Dirty, false)
            .set(Flags::Occupied, true);

        let data = self.store.get(block).await?;
        if let Some(data) = data {
            // override block
            BLOCKS_LOADED.inc();
            log::debug!("warming cache for block {block}");
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

        Ok(blk)
    }

    pub fn flush(&self) -> Result<()> {
        self.map.flush_async()?;
        Ok(())
    }

    pub fn flush_range(&self, location: usize, count: usize) -> Result<()> {
        self.map.flush_range(location, count)
    }
}

pub struct NullStore;

#[async_trait::async_trait]
impl Store for NullStore {
    async fn set(&mut self, _index: u32, _block: &[u8]) -> Result<()> {
        Ok(())
    }
    async fn get(&self, _index: u32) -> Result<Option<Data>> {
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

    use std::collections::HashMap;

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
        const PATH: &str = "/tmp/cache.reload.test";
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

        assert_eq!(cache.occupied(), 1);

        // cache can hold only 5 blocks. It already now holds 1 (block 9). If we get 5 more, block 9 should be evicted
        cache.get(0).await.unwrap();
        cache.get(1).await.unwrap();
        cache.get(2).await.unwrap();
        cache.get(3).await.unwrap();
        cache.get(4).await.unwrap();
        cache.get(5).await.unwrap();

        assert_eq!(cache.occupied(), 5);

        let mem = cache.inner();

        // while we should except 2 blocks more evicted because we
        // have pushed total of 7 blocks, but only block 9 was dirty
        // hence block 0 (the last to be evicted) is in fact not dirty
        assert_eq!(mem.mem.len(), 1);

        assert!(mem.mem.get(&9).is_some());
    }
}
