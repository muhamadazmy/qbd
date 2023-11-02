//! the cache module implements caching on top of blocks map
//! the idea is that this in memory cache can track which block
//! is stored at what index
//!
use std::{num::NonZeroU16, path::Path};

use crate::map::{Block, BlockMut, Flags, Header};

use super::map::{BlockMap, BlockSize, Error as MapError};
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
    index: usize,
    // in memory information
    // about the block can be here
}

/// Cache layer on top of BlocksMap. This allows tracking what block is in what map location
/// and make it easier to find which block in the map is least used so we can evict if needed
pub struct Cache {
    cache: LruCache<u32, CachedBlock>,
    map: BlockMap,
}

impl Cache {
    pub fn new<P: AsRef<Path>>(path: P, bc: NonZeroU16, bs: BlockSize) -> Result<Self> {
        let map = BlockMap::new(path, bc, bs)?;
        let mut cache = LruCache::new(bc.into());

        for block in map.iter() {
            println!("processing block: {}", block.index());
            let header = block.header();
            if header.flag(Flags::Occupied) {
                cache.put(
                    header.block(),
                    CachedBlock {
                        index: block.index(),
                    },
                );
            }
        }
        Ok(Self { map, cache })
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

        Some(self.map.at(item.index))
    }

    pub fn get_mut(&mut self, block: u32) -> Option<BlockMut> {
        let item = self.cache.get(&block)?;

        Some(self.map.at_mut(item.index))
    }

    pub fn put<E>(&mut self, header: Header, data: &[u8], evict: E) -> Result<()>
    where
        E: FnOnce(u32, Block) -> Result<()>,
    {
        if data.len() != self.map.block_size() {
            return Err(Error::InvalidBlockSize);
        }
        // we need to find what slot. We then need to consult the cache which slot to use !
        // right ? so either the cache is not full, then we can simply assume the next free slot
        // is the empty one! otherwise peek into cache find out what is the least used block is
        // get that out, and put that one in place!

        // when using put we always mark this block as occupied
        let header = header.with_flag(Flags::Occupied, true);

        if self.cache.len() < self.cache.cap().get() {
            // the map still has free slots then
            let mut block = self.map.at_mut(self.cache.len());
            // copy the data first! at least to invalidate
            // the crc
            block.data_mut().copy_from_slice(data);
            // set the header to given value
            block.set_header(header);
            // update the crc
            block.update_crc();

            self.cache.push(
                header.block(),
                CachedBlock {
                    index: block.index(),
                },
            );

            return Ok(());
        }

        // other wise, we need to evict one of the blocks from the map file
        // so wee peek into lru find out which one we can kick out first.

        // we know that the cache is full, so this will always return Some
        let (block, item) = self.cache.peek_lru().unwrap();
        // we need to get the block that will be evicted
        let evicted = self.map.at(item.index);

        evict(*block, evicted)?;

        // now set block
        let mut block = self.map.at_mut(item.index);
        block.data_mut().copy_from_slice(data);
        block.set_header(header);
        block.update_crc();

        self.cache.push(
            header.block(),
            CachedBlock {
                index: block.index(),
            },
        );

        Ok(())
    }
}

#[cfg(test)]
mod test {

    use std::num::NonZeroU8;

    use super::*;

    #[test]
    fn test_cache_new() {
        const PATH: &str = "/tmp/cache.test";
        let mut cache = Cache::new(
            PATH,
            NonZeroU16::new(10).unwrap(),
            BlockSize::Kilo(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

        // one kilobytes of 10s
        let data: [u8; 1024] = [10; 1024];

        //this block does not exist in the cache file yet.
        assert!(cache.get(20).is_none());

        let result = cache.put(Header::new(20), &data, |_, _| Ok(()));
        assert!(result.is_ok());

        let block = cache.get(20);
        assert!(block.is_some());

        let block = block.unwrap();
        assert!(block.is_crc_ok());
        assert_eq!(block.data()[0], 10);
    }
}
