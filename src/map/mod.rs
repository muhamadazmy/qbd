//! Page map
//! This is a map implementation that stores pages on a persisted file.
//! A map is created over a file, the file is truncated to the needed size
//! to support provided (size) and page size (page_size)
//!
//! a page is associated with a header that container information about the page
//! (id, flags, and crc)
//!
//! it's up to the user of this map to make sense of the stored values
//!
//! This works by mapping a file to memory with  mmap. The file is then split into 3 segments
//! as follows where N is number of pages
//!  - Headers section, size = N * size(u64),
//!    please check header docs
//!  - CRC section, size = N * size(u64)
//!  - DATA section, size = N * PS
//!
//! A page then is consisted of (header, crc, data) as defined by `Page`. It's up
//! to the user of the map to calculate and set CRC. Header on the other hand has
//! pre-defined values you can set (flags, id)
//! the value of the id is a u32 that is associated with that page. It is used to
//! map this page from this address, to that id on the block device (nbd)
use crate::{Error, Result};
use bytesize::ByteSize;
use memmap2::MmapMut;
use std::io::{Error as IoError, ErrorKind};
use std::{fs::OpenOptions, mem::size_of, ops::Range, os::fd::AsRawFd, path::Path};

mod header;
pub use header::{Flags, Header};
mod meta;

pub const MAX_PAGE_SIZE: ByteSize = ByteSize::mb(5);
pub const CRC: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_GO_ISO);
const FS_NOCOW_FL: i64 = 0x00800000;

pub type Crc = u64;
/// Page is a read-only page data from the cache
pub struct Page<'a> {
    address: usize,
    header: *const Header,
    data: &'a [u8],
    crc: Crc,
}

impl<'a> Page<'a> {
    /// address of this page  inside the map
    pub fn address(&self) -> usize {
        self.address
    }

    /// return header associated with page at address
    pub fn header(&self) -> &Header {
        unsafe { &*self.header }
    }

    /// verify if data and crc match
    pub fn is_crc_ok(&self) -> bool {
        self.crc == CRC.checksum(self.data())
    }

    /// returns crc stored on the page
    pub fn crc(&self) -> Crc {
        self.crc
    }

    /// data stored on the page at address
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

/// PageMut is a mut page
pub struct PageMut<'a> {
    address: usize,
    header: *mut Header,
    data: &'a mut [u8],
    crc: *mut Crc,
}

impl<'a> PageMut<'a> {
    /// address of this page  inside the map
    pub fn address(&self) -> usize {
        self.address
    }

    /// return header associated with page at location
    pub fn header(&self) -> &Header {
        unsafe { &*self.header }
    }

    /// sets header associated with page at location
    pub fn header_mut(&mut self) -> &mut Header {
        unsafe { &mut *self.header }
    }

    /// verify if data and crc match
    pub fn is_crc_ok(&self) -> bool {
        unsafe { *self.crc == CRC.checksum(self.data()) }
    }

    /// returns crc stored on the page
    pub fn crc(&self) -> Crc {
        unsafe { *self.crc }
    }

    /// updates crc to match the data
    pub fn update_crc(&mut self) {
        unsafe {
            *self.crc = CRC.checksum(self.data());
        }
    }

    /// data stored on the page at address
    pub fn data(&self) -> &[u8] {
        self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }
}

impl<'a> From<PageMut<'a>> for Page<'a> {
    fn from(value: PageMut<'a>) -> Self {
        Self {
            address: value.address,
            data: value.data,
            crc: value.crc(),
            header: value.header,
        }
    }
}

/// PageMap is an on disk cache
pub struct PageMap {
    pc: usize,
    ps: usize,
    header_rng: Range<usize>,
    crc_rng: Range<usize>,
    data_rng: Range<usize>,
    map: MmapMut,
}

impl PageMap {
    pub fn new<P: AsRef<Path>>(path: P, data_size: ByteSize, page_size: ByteSize) -> Result<Self> {
        // we need to have 3 segments in the file.
        // - header segment
        // - crc segment
        // - data segment

        let data_sec_size = data_size.as_u64() as usize;
        let ps = page_size.as_u64() as usize;

        if data_sec_size == 0 {
            return Err(Error::ZeroSize);
        }

        if ps > data_sec_size {
            return Err(Error::PageSizeTooBig);
        }

        if ps > MAX_PAGE_SIZE.as_u64() as usize {
            return Err(Error::PageSizeTooBig);
        }

        if data_sec_size % ps != 0 {
            return Err(Error::SizeNotMultipleOfPageSize);
        }

        let pc = data_sec_size / ps;

        // we can only store u32::MAX pages
        // to be able to fit it in header
        if pc > u32::MAX as usize {
            return Err(Error::PageCountTooBig);
        }

        let header_sec_size = pc * size_of::<Header>();
        let crc_sec_size = pc * size_of::<Crc>();

        // the final size is the given data size + header + crc
        let full_size = meta::SIZE + header_sec_size + crc_sec_size + data_sec_size;

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;

        let file_size = file.metadata()?.len();

        if file_size != 0 && file_size != full_size as u64 {
            return Err(Error::SizeChanged(path.as_ref().into()));
        }

        unsafe {
            let v = ioctls::fs_ioc_setflags(file.as_raw_fd(), &FS_NOCOW_FL);
            if v != 0 {
                log::error!("failed to disable COW: {v}");
            }
        }

        use nix::fcntl::{fallocate, FallocateFlags};
        // we use fallocate to allocate entire map space on disk so we grantee write operations
        // won't fail
        fallocate(
            file.as_raw_fd(),
            FallocateFlags::empty(),
            0,
            full_size as i64,
        )
        .map_err(|e| IoError::new(ErrorKind::Other, e))?;

        let mut map = unsafe { MmapMut::map_mut(&file)? };

        // validation or initializing meta section
        if file_size == 0 {
            // this is a new file. we need to set the meta
            let m = meta::Meta {
                version: meta::VERSION,
                data_size: data_size.0,
                page_size: page_size.0,
            };

            m.write(&mut map[0..meta::SIZE])?;
        } else {
            // we need to validate meta then
            let m = meta::Meta::load(&map[0..meta::SIZE])?;
            if m.version != meta::VERSION {
                return Err(Error::InvalidMetaVersion);
            }

            if m.page_size != page_size.0 {
                return Err(Error::InvalidMetaPageSize);
            }

            if m.data_size != data_size.0 {
                return Err(Error::InvalidMetaDataSize);
            }
        }

        let header_offset = meta::SIZE;
        let crc_offset = header_offset + header_sec_size;
        let data_offset = crc_offset + crc_sec_size;

        Ok(PageMap {
            pc,
            ps,
            header_rng: Range {
                start: header_offset,
                end: crc_offset,
            },
            crc_rng: Range {
                start: crc_offset,
                end: data_offset,
            },
            data_rng: Range {
                start: data_offset,
                end: full_size,
            },
            map,
        })
    }

    /// capacity of cache returns max number of pages
    pub fn page_count(&self) -> usize {
        self.pc
    }

    /// return page size of cache
    pub fn page_size(&self) -> usize {
        self.ps
    }

    fn header(&self) -> &[Header] {
        let (h, header, t) = unsafe { self.map[self.header_rng.clone()].align_to::<Header>() };
        assert!(h.is_empty(), "h is not empty");
        assert!(t.is_empty(), "t is not empty");
        header
    }

    fn crc(&self) -> &[Crc] {
        let (h, crc, t) = unsafe { self.map[self.crc_rng.clone()].align_to::<Crc>() };
        assert!(h.is_empty(), "h is not empty");
        assert!(t.is_empty(), "t is not empty");
        crc
    }

    fn data_segment(&self) -> &[u8] {
        &self.map[self.data_rng.clone()]
    }

    fn header_mut(&mut self) -> &mut [Header] {
        let (h, header, t) = unsafe { self.map[self.header_rng.clone()].align_to_mut::<Header>() };
        assert!(h.is_empty(), "h is not empty");
        assert!(t.is_empty(), "t is not empty");
        header
    }

    fn crc_mut(&mut self) -> &mut [Crc] {
        let (h, crc, t) = unsafe { self.map[self.crc_rng.clone()].align_to_mut::<Crc>() };
        assert!(h.is_empty(), "h is not empty");
        assert!(t.is_empty(), "t is not empty");
        crc
    }

    fn data_segment_mut(&mut self) -> &mut [u8] {
        &mut self.map[self.data_rng.clone()]
    }

    /// returns the offset inside the data region
    #[inline]
    fn data_block_range(&self, index: usize) -> (usize, usize) {
        let data_offset = index * self.ps;
        (data_offset, data_offset + self.ps)
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

    /// iter over all pages in cache
    pub fn iter(&self) -> impl Iterator<Item = Page> {
        PageIter {
            cache: self,
            current: 0,
        }
    }

    /// gets a page at location
    pub fn at(&self, address: usize) -> Page {
        if address >= self.pc {
            panic!("index out of range");
        }

        let data = self.data_at(address);
        let header: *const Header = self.header_at(address);
        let crc = self.crc_at(address);
        Page {
            address,
            header,
            data,
            crc,
        }
    }

    /// gets a page at location
    pub fn at_mut(&mut self, address: usize) -> PageMut {
        if address >= self.pc {
            panic!("index out of range");
        }

        let header: *mut Header = self.header_mut_at(address);
        let crc: *mut Crc = self.crc_mut_at(address);
        let data = self.data_mut_at(address);
        PageMut {
            address,
            header,
            data,
            crc,
        }
    }

    /// flush_page flushes a page and wait for it until it is written to disk
    pub fn flush_page(&self, address: usize) -> Result<()> {
        self.flush_range(address, 1)
    }

    pub fn flush_range(&self, address: usize, count: usize) -> Result<()> {
        let (mut start, _) = self.data_block_range(address);
        start += self.data_rng.start;
        let len = self.ps * count;

        // the header is also flushed but in async way
        self.map.flush_range(0, self.crc_rng.end)?;

        log::trace!("flushing page {address}/{count} [{start}: {len}]");
        self.map.flush_range(start, len).map_err(Error::from)
    }

    pub fn flush_range_async(&self, address: usize, count: usize) -> Result<()> {
        let (mut start, _) = self.data_block_range(address);
        start += self.data_rng.start;
        let len = self.ps * count;
        // the header is also flushed but in async way
        self.map.flush_range(0, self.crc_rng.end)?;

        log::trace!("flushing page {address}/{count} [{start}: {len}]");
        self.map.flush_async_range(start, len).map_err(Error::from)
    }

    /// flush a cache to disk
    pub fn flush_async(&self) -> Result<()> {
        // self.map.flush_range(offset, len)
        self.map.flush_async().map_err(Error::from)
    }
}

struct PageIter<'a> {
    cache: &'a PageMap,
    current: usize,
}

impl<'a> Iterator for PageIter<'a> {
    type Item = Page<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.cache.pc {
            return None;
        }

        let page = self.cache.at(self.current);
        self.current += 1;

        Some(page)
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
        let mut cache = PageMap::new(PATH, ByteSize::mib(10), ByteSize::mib(1)).unwrap();

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
        let cache = PageMap::new(PATH, ByteSize::mib(10), ByteSize::mib(1)).unwrap();

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
        let mut cache = PageMap::new(PATH, ByteSize::mib(10), ByteSize::mib(1)).unwrap();

        let _d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        let mut page = cache.at_mut(0);

        page.header_mut()
            .set_page(10)
            .set(header::Flags::Occupied, true)
            .set(header::Flags::Dirty, true);

        page.data_mut().fill(b'D');
        page.update_crc();

        let page = cache
            .iter()
            .filter(|b| b.header().flag(header::Flags::Dirty))
            .next();

        assert!(page.is_some());

        let page = page.unwrap();
        assert_eq!(10, page.header().page());
        assert_eq!(1024 * 1024, page.data().len());
        // all data should equal to 'D' as set above
        assert!(page.data().iter().all(|b| *b == b'D'));
    }

    #[test]
    fn test_big() {
        const PATH: &str = "/tmp/map.big.test";
        let mut cache = PageMap::new(PATH, ByteSize::gib(1), ByteSize::mib(1)).unwrap();

        let _d = Defer::new(|| {
            std::fs::remove_file(PATH).unwrap();
        });

        assert_eq!(cache.page_count(), 1024);
        // that's 1024 pages given the cache params
        for loc in 0..cache.page_count() {
            let mut page = cache.at_mut(loc);

            page.data_mut().fill_with(|| loc as u8);
            page.header_mut().set(Flags::Dirty, true);
        }

        drop(cache);

        let cache = PageMap::new(PATH, ByteSize::gib(1), ByteSize::mib(1)).unwrap();
        for loc in 0..cache.page_count() {
            let page = cache.at(loc);

            assert!(page.header().flag(Flags::Dirty));

            page.data().iter().all(|v| *v == loc as u8);
        }
    }
}
