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
use memmap2::MmapMut;
use std::{
    fs::OpenOptions,
    mem::size_of,
    num::{NonZeroU16, NonZeroU8},
    ops::Range,
    path::Path,
};

mod header;
pub use header::{Flags, Header};

pub const CRC: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_GO_ISO);

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io error: {0}")]
    IO(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub type Crc = u64;

/// BlockSize enum. This is to force a limit on the block size that
/// can be requests. This way maximum size that can be provided is 512 MB
pub enum BlockSize {
    Kilo(NonZeroU8),
    Mega(NonZeroU8),
}

impl BlockSize {
    fn bytes(&self) -> u64 {
        match &self {
            Self::Kilo(v) => v.get() as u64 * 1024,
            Self::Mega(v) => v.get() as u64 * 1024 * 1024,
        }
    }
}

/// Block is a read-only block data from the cache
pub struct Block<'a> {
    cache: &'a BlockMap,
    index: usize,
}

impl<'a> Block<'a> {
    /// index of the block, this is the block
    /// position in the memmap.
    pub fn index(&self) -> usize {
        self.index
    }

    /// header associated with the block
    pub fn header(&self) -> Header {
        self.cache.header()[self.index]
    }

    pub fn is_crc_ok(&self) -> bool {
        self.cache.crc()[self.index] == CRC.checksum(self.data())
    }

    /// returns crc stored on the block
    pub fn crc(&self) -> Crc {
        self.cache.crc()[self.index]
    }

    /// data bytes
    pub fn data(&self) -> &[u8] {
        self.cache.data_block(self.index)
    }
}

/// BlockMut is a mut block
pub struct BlockMut<'a> {
    cache: &'a mut BlockMap,
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

    pub fn is_crc_ok(&self) -> bool {
        self.cache.crc()[self.index] == CRC.checksum(self.data())
    }

    /// returns crc stored on the block
    pub fn crc(&self) -> Crc {
        self.cache.crc()[self.index]
    }
    /// updates crc to match the data
    pub fn update_crc(&mut self) {
        self.cache.crc_mut()[self.index] = CRC.checksum(&self.data())
    }

    pub fn data(&self) -> &[u8] {
        self.cache.data_block(self.index)
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.cache.data_block_mut(self.index)
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

    pub fn purge(&mut self) -> Result<()> {
        self.map.fill(0);
        self.flush()
    }

    pub fn flush(&self) -> Result<()> {
        self.map.flush_async().map_err(Error::from)
    }

    /// capacity of cache returns max number of blocks
    pub fn cap(&self) -> usize {
        self.bc
    }

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
        let mut cache = BlockMap::new(
            PATH,
            NonZeroU16::new(10).unwrap(),
            BlockSize::Mega(NonZeroU8::new(1).unwrap()),
        )
        .unwrap();

        let d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        let header = cache.header_mut();
        assert_eq!(10, header.len());
        header.fill(10.into());

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
        let cache = BlockMap::new(
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
        let mut cache = BlockMap::new(
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
            Header::new(10)
                .with_flag(header::Flags::Occupied, true)
                .with_flag(header::Flags::Dirty, true),
        );

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
