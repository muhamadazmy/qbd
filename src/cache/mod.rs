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
    pub index: usize,
    pub header: Header,
    pub crc: Crc,
    pub block: &'a [u8],
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

    fn data(&self) -> &[u8] {
        &self.map[self.data_rng.clone()]
    }

    fn header_mut(&mut self) -> &mut [Header] {
        unsafe { std::mem::transmute(&mut self.map[self.header_rng.clone()]) }
    }

    fn crc_mut(&mut self) -> &mut [Crc] {
        unsafe { std::mem::transmute(&mut self.map[self.crc_rng.clone()]) }
    }

    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.map[self.data_rng.clone()]
    }

    pub fn iter(&self) -> CacheIter {
        CacheIter {
            cache: self,
            current: 0,
        }
    }

    pub fn at(&self, index: usize) -> Block {
        if index >= self.bc {
            panic!("index out of range");
        }

        let data_offset = index * self.bs;

        let data = &self.data()[data_offset..data_offset + self.bs];

        //let data = &self.cache.data()[offset.. off]
        Block {
            index: index,
            header: self.header()[index],
            crc: self.crc()[index],
            block: data,
        }
    }

    pub fn at_mut(&mut self, index: usize) {
        // return a mut block. a mut block then
        // should allow caller to change flags, CRC or data
        // todo:
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
        let mut cache = BlockCache::new(
            "/tmp/segments.test",
            NonZeroU16::new(10).unwrap(),
            BlockSize::Mega(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

        let d = Defer::new(|| {
            std::fs::remove_file("/tmp/segments.test");
        });

        let header = cache.header_mut();
        header.fill(10.into());
        let crc = cache.crc_mut();
        crc.fill(20);
        let data = cache.data_mut();
        data.fill(b'D');

        let header = cache.header();
        let crc = cache.crc();
        let data = cache.data();

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
        let cache = BlockCache::new(
            "/tmp/iter.test",
            NonZeroU16::new(10).unwrap(),
            BlockSize::Mega(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

        let d = Defer::new(|| {
            std::fs::remove_file("/tmp/iter.test");
        });

        assert_eq!(10, cache.iter().count());

        assert_eq!(
            0,
            cache
                .iter()
                .filter(|b| b.header.flag(header::Flags::Dirty))
                .count()
        );
    }
}
