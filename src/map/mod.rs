//! Block map
//! This is a map implementation that stores blocks on a persisted file.
//! A map is created over a file, the file is truncated to the needed size
//! to support provided block count (bc) and block size (bs)
//!
//! a block is associated with a header that container information about the block
//! (id, flags, and crc)
//!
//! it's up to the user of this map to make sense of the stored values
//!
//! This works by mapping a file to a memmap. The file is then split into 3 segments
//! as follows where N is number of blocks
//!  - Headers section, size = N * size(u64),
//!    please check header docs
//!  - CRC section, size = N * size(u64)
//!  - DATA section, size = N * BS
//!
//! A block then is consisted of (header, crc, data) as defined by `Block`. It's up
//! to the user of the map to calculate and set CRC. Header on the other hand has
//! pre-defined values you can set (flags, id) but the value of `id`
use bytesize::ByteSize;
use memmap2::MmapMut;
use std::{fs::OpenOptions, mem::size_of, ops::Range, os::fd::AsRawFd, path::Path};

mod header;
use crate::{Error, Result};
pub use header::{Flags, Header};

pub const MAX_BLOCK_SIZE: ByteSize = ByteSize::mb(5);
pub const CRC: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_GO_ISO);
const FS_NOCOW_FL: i64 = 0x00800000;

pub type Crc = u64;
/// Block is a read-only block data from the cache
pub struct Block<'a> {
    location: usize,
    header: *const Header,
    data: &'a [u8],
    crc: Crc,
}

impl<'a> Block<'a> {
    /// location of the block in cache
    pub fn location(&self) -> usize {
        self.location
    }

    /// header associated with the block
    pub fn header(&self) -> &Header {
        unsafe { &*self.header }
    }

    /// verify if data and crc match
    pub fn is_crc_ok(&self) -> bool {
        self.crc == CRC.checksum(self.data())
    }

    /// returns crc stored on the block
    pub fn crc(&self) -> Crc {
        self.crc
    }

    /// data bytes stored on block at location
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

/// BlockMut is a mut block
pub struct BlockMut<'a> {
    location: usize,
    header: *mut Header,
    data: &'a mut [u8],
    crc: *mut Crc,
}

impl<'a> BlockMut<'a> {
    /// location is the block location inside the cache
    pub fn location(&self) -> usize {
        self.location
    }

    /// return header associated with block at location
    pub fn header(&self) -> &Header {
        unsafe { &*self.header }
    }

    /// sets header associated with block at location
    pub fn header_mut(&mut self) -> &mut Header {
        unsafe { &mut *self.header }
    }

    /// verify if data and crc match
    pub fn is_crc_ok(&self) -> bool {
        unsafe { *self.crc == CRC.checksum(self.data()) }
    }

    /// returns crc stored on the block
    pub fn crc(&self) -> Crc {
        unsafe { *self.crc }
    }

    /// updates crc to match the data
    pub fn update_crc(&mut self) {
        unsafe {
            *self.crc = CRC.checksum(self.data());
        }
    }

    /// data stored on the block at location
    pub fn data(&self) -> &[u8] {
        self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }
}

impl<'a> From<BlockMut<'a>> for Block<'a> {
    fn from(value: BlockMut<'a>) -> Self {
        Self {
            location: value.location,
            data: value.data,
            crc: value.crc(),
            header: value.header,
        }
    }
}
/// BlockCache is an on disk cache
pub struct BlockMap {
    bc: usize,
    bs: usize,
    header_rng: Range<usize>,
    crc_rng: Range<usize>,
    data_rng: Range<usize>,
    map: MmapMut,
}

impl BlockMap {
    pub fn new<P: AsRef<Path>>(path: P, size: ByteSize, bs: ByteSize) -> Result<Self> {
        // we need to have 3 segments in the file.
        // - header segment
        // - crc segment
        // - data segment

        let size = size.as_u64() as usize;
        let bs = bs.as_u64() as usize;

        if size == 0 {
            return Err(Error::ZeroSize);
        }

        if bs > size {
            return Err(Error::BlockSizeTooBig);
        }

        if bs > MAX_BLOCK_SIZE.as_u64() as usize {
            return Err(Error::BlockSizeTooBig);
        }

        if size % bs != 0 {
            return Err(Error::SizeNotMultipleOfBlockSize);
        }

        let bc = size / bs;

        // // we can only store u32::MAX blocks
        // // to be able to fit it in header
        // if bc > u32::MAX as usize {
        //     return Err(Error::BlockCountTooBig);
        // }

        let header = bc * size_of::<Header>();
        let crc = bc * size_of::<Crc>();

        // the final size is the given data size + header + crc
        let size = size + header + crc;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;

        unsafe {
            let v = ioctls::fs_ioc_setflags(file.as_raw_fd(), &FS_NOCOW_FL);
            if v != 0 {
                log::error!("failed to disable COW: {v}");
            }
        }

        file.set_len(size as u64)?;
        // we need then to open the underlying file and truncate it
        Ok(BlockMap {
            bc,
            bs,
            header_rng: Range {
                start: 0,
                end: header,
            },
            crc_rng: Range {
                start: header,
                end: header + crc,
            },
            data_rng: Range {
                start: header + crc,
                end: size,
            },
            map: unsafe { MmapMut::map_mut(&file)? },
        })
    }

    /// capacity of cache returns max number of blocks
    pub fn block_count(&self) -> usize {
        self.bc
    }

    /// return block size of cache
    pub fn block_size(&self) -> usize {
        self.bs
    }

    fn header(&self) -> &[Header] {
        let (_, header, _) = unsafe { self.map[self.header_rng.clone()].align_to::<Header>() };
        header
    }

    fn crc(&self) -> &[Crc] {
        let (_, crc, _) = unsafe { self.map[self.crc_rng.clone()].align_to::<Crc>() };
        crc
    }

    fn data_segment(&self) -> &[u8] {
        &self.map[self.data_rng.clone()]
    }

    fn header_mut(&mut self) -> &mut [Header] {
        let (_, header, _) = unsafe { self.map[self.header_rng.clone()].align_to_mut::<Header>() };
        header
    }

    fn crc_mut(&mut self) -> &mut [Crc] {
        let (_, crc, _) = unsafe { self.map[self.crc_rng.clone()].align_to_mut::<Crc>() };
        crc
    }

    fn data_segment_mut(&mut self) -> &mut [u8] {
        &mut self.map[self.data_rng.clone()]
    }

    /// returns the offset inside the data region
    #[inline]
    fn data_block_range(&self, index: usize) -> (usize, usize) {
        let data_offset = index * self.bs;
        (data_offset, data_offset + self.bs)
    }

    #[inline]
    pub(crate) fn data_at(&self, index: usize) -> &[u8] {
        let (start, end) = self.data_block_range(index);
        &self.data_segment()[start..end]
    }

    #[inline]
    pub(crate) fn data_mut_at(&mut self, index: usize) -> &mut [u8] {
        let (start, end) = self.data_block_range(index);
        &mut self.data_segment_mut()[start..end]
    }

    #[inline]
    pub(crate) fn header_at(&self, index: usize) -> &Header {
        &self.header()[index]
    }

    #[inline]
    pub(crate) fn header_mut_at(&mut self, index: usize) -> &mut Header {
        &mut self.header_mut()[index]
    }

    #[inline]
    pub(crate) fn crc_at(&self, index: usize) -> Crc {
        self.crc()[index]
    }

    #[inline]
    pub(crate) fn crc_mut_at(&mut self, index: usize) -> &mut Crc {
        &mut self.crc_mut()[index]
    }

    /// iter over all blocks in cache
    pub fn iter(&self) -> impl Iterator<Item = Block> {
        CacheIter {
            cache: self,
            current: 0,
        }
    }

    /// gets a block at location
    pub fn at(&self, location: usize) -> Block {
        if location >= self.bc {
            panic!("index out of range");
        }

        let data = self.data_at(location);
        let header: *const Header = self.header_at(location);
        let crc = self.crc_at(location);
        Block {
            location,
            header,
            data,
            crc,
        }
    }

    /// gets a block_mut at location
    pub fn at_mut(&mut self, location: usize) -> BlockMut {
        if location >= self.bc {
            panic!("index out of range");
        }

        let header: *mut Header = self.header_mut_at(location);
        let crc: *mut Crc = self.crc_mut_at(location);
        let data = self.data_mut_at(location);
        BlockMut {
            location,
            header,
            data,
            crc,
        }
    }

    /// flush_block flushes a block and wait for it until it is written to disk
    pub fn flush_block(&self, location: usize) -> Result<()> {
        self.flush_range(location, 1)
    }

    pub fn flush_range(&self, location: usize, count: usize) -> Result<()> {
        let (mut start, _) = self.data_block_range(location);
        start += self.data_rng.start;
        let len = self.bs * count;
        log::debug!("flushing block {location}/{count} [{start}: {len}]");
        self.map.flush_async_range(start, len)?;

        // the header is also flushed but in async way
        self.map
            .flush_async_range(0, self.crc_rng.end)
            .map_err(Error::from)
    }

    /// flush a cache to disk
    pub fn flush_async(&self) -> Result<()> {
        // self.map.flush_range(offset, len)
        self.map.flush_async().map_err(Error::from)
    }
}

struct CacheIter<'a> {
    cache: &'a BlockMap,
    current: usize,
}

impl<'a> Iterator for CacheIter<'a> {
    type Item = Block<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.cache.bc {
            return None;
        }

        let block = self.cache.at(self.current);
        self.current += 1;

        Some(block)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct Defer<F>(Option<F>)
    where
        F: FnOnce();

    impl<F> Defer<F>
    where
        F: FnOnce(),
    {
        fn new(f: F) -> Self {
            Self(Some(f))
        }
    }

    impl<F> Drop for Defer<F>
    where
        F: FnOnce(),
    {
        fn drop(&mut self) {
            self.0.take().map(|f| f());
        }
    }

    #[test]
    fn segments() {
        const PATH: &str = "/tmp/segments.test";
        let mut cache = BlockMap::new(PATH, ByteSize::mib(10), ByteSize::mib(1)).unwrap();

        let _d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        let header = cache.header_mut();
        assert_eq!(10, header.len());
        header.fill(Header::new(10));

        let crc = cache.crc_mut();
        assert_eq!(10, crc.len());
        crc.fill(20);

        let data = cache.data_segment_mut();
        data.fill(b'D');
        assert_eq!(10 * 1024 * 1024, data.len());

        let header = cache.header();
        let crc = cache.crc();
        let data = cache.data_segment();

        assert_eq!(10, header.len());
        assert_eq!(10, crc.len());
        assert_eq!(10 * 1024 * 1024, data.len());

        for c in header.iter() {
            assert_eq!(*c, Header::new(10));
        }
        for c in crc.iter() {
            assert_eq!(*c, 20);
        }
        for c in data.iter() {
            assert_eq!(*c, b'D');
        }
    }

    #[test]
    fn iterator() {
        const PATH: &str = "/tmp/iter.test";
        let cache = BlockMap::new(PATH, ByteSize::mib(10), ByteSize::mib(1)).unwrap();

        let _d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        assert_eq!(10, cache.iter().count());

        assert_eq!(
            0,
            cache
                .iter()
                .filter(|b| b.header().flag(header::Flags::Dirty))
                .count()
        );
    }

    #[test]
    fn edit() {
        const PATH: &str = "/tmp/edit.test";
        let mut cache = BlockMap::new(PATH, ByteSize::mib(10), ByteSize::mib(1)).unwrap();

        let _d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        let mut block = cache.at_mut(0);

        block
            .header_mut()
            .set_block(10)
            .set(header::Flags::Occupied, true)
            .set(header::Flags::Dirty, true);

        block.data_mut().fill(b'D');
        block.update_crc();

        let block = cache
            .iter()
            .filter(|b| b.header().flag(header::Flags::Dirty))
            .next();

        assert!(block.is_some());

        let block = block.unwrap();
        assert_eq!(10, block.header().block());
        assert_eq!(1024 * 1024, block.data().len());
        // all data should equal to 'D' as set above
        assert!(block.data().iter().all(|b| *b == b'D'));
    }
}
