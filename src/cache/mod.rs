//! Block cache
//! This is a cache implementation that caches blocks on a persisted file.
//! A cache is created over a file, the file is truncated to the needed size
//! to support provided block count (bc) and block size (bs)
//!
use memmap2::MmapMut;
use std::{
    fs::OpenOptions,
    mem::size_of,
    num::{NonZeroU16, NonZeroU8},
    ops::Range,
    path::{Path, PathBuf},
};

mod header;
pub use header::Header;

#[derive(thiserror::Error, Debug)]
pub enum BlockCacheError {
    #[error("io error: {0}")]
    IO(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, BlockCacheError>;

type Crc = u64;

pub enum BlockSize {
    Mega(NonZeroU8),
}

impl BlockSize {
    fn bytes(&self) -> u64 {
        match &self {
            Self::Mega(v) => v.get() as u64 * 1024 * 1024,
        }
    }
}

pub struct Block<'a> {
    cache: &'a BlockCache,
    index: usize,
}

impl<'a> Block<'a> {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn header(&self) -> Header {
        self.cache.header()[self.index]
    }

    pub fn crc(&self) -> Crc {
        self.cache.crc()[self.index]
    }

    pub fn data(&self) -> &[u8] {
        self.cache.data_block(self.index)
    }
}

pub struct BlockMut<'a> {
    cache: &'a mut BlockCache,
    index: usize,
}

impl<'a> BlockMut<'a> {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn header(&self) -> Header {
        self.cache.header()[self.index]
    }

    pub fn set_header(&mut self, header: Header) {
        self.cache.header_mut()[self.index] = header;
    }

    pub fn crc(&self) -> Crc {
        self.cache.crc()[self.index]
    }

    pub fn set_crc(&mut self, crc: Crc) {
        self.cache.crc_mut()[self.index] = crc;
    }

    pub fn data(&self) -> &[u8] {
        self.cache.data_block(self.index)
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.cache.data_block_mut(self.index)
    }
}

pub struct BlockCache {
    bc: usize,
    bs: usize,
    header_rng: Range<usize>,
    crc_rng: Range<usize>,
    data_rng: Range<usize>,
    map: MmapMut,
}

impl BlockCache {
    pub fn new<P: AsRef<Path>>(path: P, bc: NonZeroU16, bs: BlockSize) -> Result<Self> {
        // we need to have 3 segments in the file.
        // - header segment
        // - crc segment
        // - data segment

        let bc = bc.get() as usize;
        let bs = bs.bytes() as usize;

        let header = bc * size_of::<Header>();
        let crc = bc * size_of::<Crc>();
        let data = bc * bs;

        let size = header + crc + data;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;

        file.set_len(size as u64)?;
        // we need then to open the underlying file and truncate it
        Ok(BlockCache {
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

    pub fn purge(&mut self) -> Result<()> {
        self.map.fill(0);
        self.flush()
    }

    pub fn flush(&self) -> Result<()> {
        self.map.flush_async().map_err(BlockCacheError::from)
    }

    /// capacity of cache returns max number of blocks
    pub fn cap(&self) -> usize {
        self.bc
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
        unsafe { std::mem::transmute(&mut self.map[self.header_rng.clone()]) }
    }

    fn crc_mut(&mut self) -> &mut [Crc] {
        unsafe { std::mem::transmute(&mut self.map[self.crc_rng.clone()]) }
    }

    fn data_segment_mut(&mut self) -> &mut [u8] {
        &mut self.map[self.data_rng.clone()]
    }

    fn data_block(&self, index: usize) -> &[u8] {
        let data_offset = index * self.bs;
        &self.data_segment()[data_offset..data_offset + self.bs]
    }

    fn data_block_mut(&mut self, index: usize) -> &mut [u8] {
        let data_offset = index * self.bs;
        let range = data_offset..data_offset + self.bs;
        &mut self.data_segment_mut()[range]
    }

    pub fn iter(&self) -> impl Iterator<Item = Block> {
        CacheIter {
            cache: self,
            current: 0,
        }
    }

    pub fn at(&self, index: usize) -> Block {
        if index >= self.bc {
            panic!("index out of range");
        }

        //let data = &self.cache.data()[offset.. off]
        Block {
            cache: self,
            index: index,
        }
    }

    pub fn at_mut(&mut self, index: usize) -> BlockMut {
        // return a mut block. a mut block then
        // should allow caller to change flags, CRC or data
        // todo:
        if index >= self.bc {
            panic!("index out of range");
        }

        BlockMut {
            cache: self,
            index: index,
        }
    }
}

struct CacheIter<'a> {
    cache: &'a BlockCache,
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

// struct CacheIterMut<'a> {
//     cache: &'a mut BlockCache,
//     current: usize,
// }

// impl<'a> Iterator for CacheIterMut<'a> {
//     type Item = BlockMut<'a>;
//     fn next(&mut self) -> Option<Self::Item> {
//         if self.current == self.cache.bc {
//             return None;
//         }

//         let block = self.cache.at_mut(self.current);
//         self.current += 1;

//         Some(block)
//     }
// }

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
        let mut cache = BlockCache::new(
            PATH,
            NonZeroU16::new(10).unwrap(),
            BlockSize::Mega(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

        let d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        let header = cache.header_mut();
        header.fill(10.into());
        let crc = cache.crc_mut();
        crc.fill(20);
        let data = cache.data_segment_mut();
        data.fill(b'D');

        let header = cache.header();
        let crc = cache.crc();
        let data = cache.data_segment();

        assert_eq!(10, header.len());
        assert_eq!(10, crc.len());
        assert_eq!(10 * 1024 * 1024, data.len());

        for c in header.iter() {
            assert_eq!(*c, Header::from(10));
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
        let cache = BlockCache::new(
            PATH,
            NonZeroU16::new(10).unwrap(),
            BlockSize::Mega(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

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
        let mut cache = BlockCache::new(
            PATH,
            NonZeroU16::new(10).unwrap(),
            BlockSize::Mega(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

        let d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        let mut block = cache.at_mut(0);

        block.set_header(
            Header::default()
                .with_id(10)
                .with_flag(header::Flags::Occupied, true)
                .with_flag(header::Flags::Dirty, true),
        );

        block.data_mut().fill(b'D');
        block.set_crc(1000);

        let block = cache
            .iter()
            .filter(|b| b.header().flag(header::Flags::Dirty))
            .next();

        assert!(block.is_some());

        let block = block.unwrap();
        assert_eq!(10, block.header().id());
        assert_eq!(1024 * 1024, block.data().len());
        // all data should equal to 'D' as set above
        assert!(block.data().iter().all(|b| *b == b'D'));
    }
}
