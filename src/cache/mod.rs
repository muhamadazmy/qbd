//! the cache module implements caching on top of blocks map
//! the idea is that this in memory cache can track which block
//! is stored at what index
//!
//! the block index used in get/put operations are the full range
//! of blocks supported by the nbd device. If using block size of
//! 1MiB this maps to 4096TiB
//!
use std::{num::NonZeroUsize, path::Path};

use crate::map::{Block, BlockMut, Flags, Header};

use super::map::{BlockMap, Error as MapError};
use bytesize::ByteSize;
use lru::LruCache;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid input block size expected")]
    InvalidBlockSize,

    #[error("map error: {0}")]
    MapError(#[from] MapError),
}

pub type Result<T> = std::result::Result<T, Error>;
/// CachedBlock holds information about blocks in lru memory
struct CachedBlock {
    /// location of the block in underlying cache
    location: usize,
    // in memory information
    // about the block can be here
}

impl From<Error> for std::io::Error {
    fn from(value: Error) -> Self {
        use std::io::{Error as IoError, ErrorKind};

        // TODO: possible different error kind
        match value {
            Error::MapError(MapError::IO(err)) => err,
            _ => IoError::new(ErrorKind::InvalidInput, value),
        }
    }
}

#[async_trait::async_trait]
pub trait EvictSink {
    async fn evict(&mut self, index: u32, block: Block<'_>) -> Result<()>;
}

/// Cache layer on top of BlockMap. This allows tracking what block is in what map location
/// and make it easier to find which block in the map is least used so we can evict if needed
pub struct Cache {
    cache: LruCache<u32, CachedBlock>,
    map: BlockMap,
    bc: u64,
    bs: u64,
}

impl Cache {
    pub fn new<P: AsRef<Path>>(path: P, size: ByteSize, bs: ByteSize) -> Result<Self> {
        let map = BlockMap::new(path, size, bs)?;
        let bc = size.as_u64() / bs.as_u64();

        let mut cache = LruCache::new(NonZeroUsize::new(bc as usize).ok_or(MapError::ZeroSize)?);

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
        Ok(Self {
            map,
            cache,
            bc,
            bs: bs.as_u64(),
        })
    }

    pub fn block_size(&self) -> u64 {
        self.bs
    }

    pub fn block_count(&self) -> u64 {
        self.bc
    }

    /// gets the block with id <block> if already in cache, other wise return None
    /// TODO: enhance access to this method. the `mut` is only needed to allow
    /// the lru cache to update, but the block itself doesn't need it because it
    /// requires no mut borrowing. But then multiple calls to get won't be possible
    /// because i will need exclusive access to this, which will slow down read
    /// access.
    pub fn get(&mut self, block: u32) -> Option<Block> {
        // we first hit the mem cache see if there is a block tracked here
        let item = self.cache.get(&block)?;

        Some(self.map.at(item.location))
    }

    /// get a BlockMut
    pub fn get_mut(&mut self, block: u32) -> Option<BlockMut> {
        let item = self.cache.get(&block)?;

        Some(self.map.at_mut(item.location))
    }

    /// puts a block into cache, if the put operation requires eviction of a colder block, the cold
    /// the evict function will be called with that evicted block.
    /// If optional data is provided the data will be written as well to the new block.
    ///
    /// on success the BlockMut is returned
    pub async fn put<E>(
        &mut self,
        header: Header,
        data: Option<&[u8]>,
        sink: &mut E,
    ) -> Result<BlockMut>
    where
        E: EvictSink,
    {
        if let Some(data) = data {
            if data.len() != self.map.block_size() {
                return Err(Error::InvalidBlockSize);
            }
        }
        // we need to find what slot. We then need to consult the cache which slot to use !
        // right ? so either the cache is not full, then we can simply assume the next free slot
        // is the empty one! otherwise peek into cache find out what is the least used block is
        // get that out, and put that one in place!

        // when using put we always mark this block as occupied
        let header = header.with_flag(Flags::Occupied, true);

        let mut block: BlockMut;
        if self.cache.len() < self.cache.cap().get() {
            // the map still has free slots then
            block = self.map.at_mut(self.cache.len());
        } else {
            // other wise, we need to evict one of the blocks from the map file
            // so wee peek into lru find out which one we can kick out first.

            // we know that the cache is full, so this will always return Some
            let (block_index, item) = self.cache.peek_lru().unwrap();
            // we need to get the block that will be evicted
            let evicted = self.map.at(item.location);

            sink.evict(*block_index, evicted).await?;

            // now set block
            block = self.map.at_mut(item.location);
        }

        if let Some(data) = data {
            block.data_mut().copy_from_slice(data);
            block.update_crc();
        }

        block.set_header(header);

        self.cache.push(
            header.block(),
            CachedBlock {
                location: block.location(),
            },
        );

        Ok(block)
    }

    pub fn flush(&self) -> Result<()> {
        self.map.flush_async().map_err(Error::MapError)
    }
}

pub struct EvictNoop;

#[async_trait::async_trait]
impl EvictSink for EvictNoop {
    async fn evict(&mut self, _: u32, _: Block<'_>) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[tokio::test]
    async fn test_cache_new() {
        const PATH: &str = "/tmp/cache.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let mut cache = Cache::new(PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        // one kilobytes of 10s
        let data: [u8; 1024] = [10; 1024];

        //this block does not exist in the cache file yet.
        assert!(cache.get(20).is_none());

        let result = cache
            .put(Header::new(20), Some(&data), &mut EvictNoop)
            .await;
        assert!(result.is_ok());

        let block = cache.get(20);
        assert!(block.is_some());

        let block = block.unwrap();
        assert!(block.is_crc_ok());
        assert_eq!(block.data()[0], 10);
    }

    #[tokio::test]
    async fn test_cache_reload() {
        const PATH: &str = "/tmp/cache.reload.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let mut cache = Cache::new(PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        // one kilobytes of 10s
        let data: [u8; 1024] = [10; 1024];

        //this block does not exist in the cache file yet.
        assert!(cache.get(20).is_none());

        let result = cache
            .put(Header::new(20), Some(&data), &mut EvictNoop)
            .await;
        assert!(result.is_ok());

        // drop cache
        drop(cache);

        let mut cache = Cache::new(PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        let block = cache.get(20);
        assert!(block.is_some());

        let block = block.unwrap();
        assert!(block.is_crc_ok());
        assert_eq!(block.data()[0], 10);
    }
}
