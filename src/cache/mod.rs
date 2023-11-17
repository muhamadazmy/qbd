//! the cache module implements caching on top of blocks map
//! the idea is that this in memory cache can track which block
//! is stored at what index
//!
//! the block index used in get/put operations are the full range
//! of blocks supported by the nbd device. If using block size of
//! 1MiB this maps to 4096TiB
//!
use std::{
    num::NonZeroUsize,
    path::Path,
    time::{Duration, Instant},
};

use crate::{
    map::{Flags, Page, PageMut},
    store::{Page as PageData, Store},
};

use super::map::PageMap;
use bytesize::ByteSize;
use lazy_static::lazy_static;
use lru::LruCache;
use prometheus::{
    register_histogram, register_int_counter, register_int_gauge, Histogram, IntCounter, IntGauge,
};

use crate::{Error, Result};

lazy_static! {
    static ref PAGES_EVICTED: IntCounter =
        register_int_counter!("nbd_pages_evicted", "number of pages evicted to backend").unwrap();
    static ref PAGES_LOADED: IntCounter =
        register_int_counter!("nbd_pages_loaded", "number of pages loaded from backend").unwrap();
    static ref PAGES_CACHED: IntGauge =
        register_int_gauge!("nbd_pages_cached", "number of pages available in cache").unwrap();
    static ref EVICT_HISTOGRAM: Histogram = register_histogram!(
        "nbd_evict_histogram",
        "page eviction histogram",
        vec![0.001, 0.010, 0.050, 0.100, 0.250, 0.500]
    )
    .unwrap();
    static ref LOAD_HISTOGRAM: Histogram = register_histogram!(
        "nbd_load_histogram",
        "load eviction histogram",
        vec![0.001, 0.010, 0.050, 0.100, 0.250, 0.500]
    )
    .unwrap();
}

/// CachedBlock holds information about blocks in lru memory
struct CachedPage {
    /// address of the block in underlying cache
    address: usize,
    // in memory information
    // about the block can be here
}

/// Cache layer on top of BlockMap. This allows tracking what block is in what map location
/// and make it easier to find which block in the map is least used so we can evict if needed
pub struct Cache<S>
where
    S: Store,
{
    cache: LruCache<u32, CachedPage>,
    map: PageMap,
    store: S,
    // blocks is number of possible blocks
    // in the store (store.size() / bs)
    pages: usize,
}

impl<S> Cache<S>
where
    S: Store,
{
    pub fn new<P: AsRef<Path>>(
        store: S,
        path: P,
        size: ByteSize,
        page_size: ByteSize,
    ) -> Result<Self> {
        let map = PageMap::new(path, size, page_size)?;
        let pc = size.as_u64() / page_size.as_u64();

        let mut cache = LruCache::new(NonZeroUsize::new(pc as usize).ok_or(Error::ZeroSize)?);

        for page in map.iter() {
            let header = page.header();
            if header.flag(Flags::Occupied) {
                cache.put(
                    header.page(),
                    CachedPage {
                        address: page.address(),
                    },
                );
            }
        }

        PAGES_CACHED.set(cache.len() as i64);
        // to be able to check block boundaries
        let pages = store.size().as_u64() / page_size.as_u64();
        log::debug!("device pages: {pages}");
        Ok(Self {
            map,
            cache,
            store,
            pages: pages as usize,
        })
    }

    pub fn inner(self) -> S {
        self.store
    }

    pub fn page_size(&self) -> usize {
        self.map.page_size()
    }

    pub fn page_count(&self) -> usize {
        self.map.page_count()
    }

    pub fn occupied(&self) -> usize {
        self.map
            .iter()
            .filter(|b| b.header().flag(Flags::Occupied))
            .count()
    }
    /// gets the page with index <page> if already in cache, other wise return None
    /// TODO: enhance access to this method. the `mut` is only needed to allow
    /// the lru cache to update, but the block itself doesn't need it because it
    /// requires no mut borrowing. But then multiple calls to get won't be possible
    /// because i will need exclusive access to this, which will slow down read
    /// access.
    pub async fn get(&mut self, page: u32) -> Result<Page> {
        // we first hit the mem cache see if there is a block tracked here
        if page as usize >= self.pages {
            return Err(Error::PageIndexOutOfRange);
        }
        let item = self.cache.get(&page);
        match item {
            Some(cached) => Ok(self.map.at(cached.address)),
            None => self.warm(page).await.map(Page::from),
        }
    }

    /// get a BlockMut
    pub async fn get_mut(&mut self, page: u32) -> Result<PageMut> {
        if page as usize >= self.pages {
            return Err(Error::PageIndexOutOfRange);
        }

        let item = self.cache.get(&page);
        match item {
            Some(cached) => Ok(self.map.at_mut(cached.address)),
            None => self.warm(page).await,
        }
    }

    async fn warm(&mut self, page: u32) -> Result<PageMut> {
        // first find which block to evict.

        let mut pge: PageMut;
        if self.cache.len() < self.cache.cap().get() {
            // the map still has free slots then
            pge = self.map.at_mut(self.cache.len());
        } else {
            // other wise, we need to evict one of the blocks from the map file
            // so wee peek into lru find out which one we can kick out first.

            // we know that the cache is full, so this will always return Some
            let (page_index, item) = self.cache.peek_lru().unwrap();
            // so block block_index stored at map location item.location
            // can be evicted
            pge = self.map.at_mut(item.address);

            // store this in permanent store
            // eviction should only happen if blk is dirty
            // note it's up to user of the cache to mark blocks as
            // dirty otherwise they won't evict to backend
            if pge.header().flag(Flags::Dirty) {
                log::debug!("page {} eviction", *page_index);
                PAGES_EVICTED.inc();
                let timer = EVICT_HISTOGRAM.start_timer();
                self.store.set(*page_index, pge.data()).await?;
                timer.observe_duration();
            } else {
                log::trace!("block {} eviction skipped", *page_index);
            }

            // now the block location is ready to be reuse
            // note that the next call to push will actually remove that item from the lru
        }

        pge.header_mut()
            .set_page(page)
            .set(Flags::Dirty, false)
            .set(Flags::Occupied, true);

        assert_eq!(pge.header().page(), page, "page header update");
        let timer = LOAD_HISTOGRAM.start_timer();
        let data = self.store.get(page).await?;
        timer.observe_duration();
        if let Some(data) = data {
            // override block
            PAGES_LOADED.inc();
            log::trace!("warming cache for block {page}");
            pge.data_mut().copy_from_slice(&data);
            pge.update_crc();
        } else {
            // should we zero it out ?
            // or not
        }

        self.cache.push(
            page,
            CachedPage {
                address: pge.address(),
            },
        );

        PAGES_CACHED.set(self.cache.len() as i64);
        Ok(pge)
    }

    pub fn flush(&self) -> Result<()> {
        self.map.flush_async()?;
        Ok(())
    }

    pub fn flush_range(&self, location: usize, count: usize) -> Result<()> {
        self.map.flush_range_async(location, count)
    }

    // try evicting whatever it can in no_longer_than
    pub async fn evict(&mut self, no_longer_than: Duration) -> Result<()> {
        let start = Instant::now();
        for (page_index, cached) in self.cache.iter().rev() {
            log::trace!("check page {} for eviction", *page_index);
            let mut page = self.map.at_mut(cached.address);
            if page.header().flag(Flags::Dirty) {
                PAGES_EVICTED.inc();
                log::trace!("background eviction of {}", *page_index);
                self.store.set(*page_index, page.data()).await?;
                page.header_mut().set(Flags::Dirty, false);
            }

            if start.elapsed() > no_longer_than {
                return Ok(());
            }
        }

        Ok(())
    }
}

pub struct NullStore;

#[async_trait::async_trait]
impl Store for NullStore {
    type Vec = Vec<u8>;

    async fn set(&mut self, _index: u32, _block: &[u8]) -> Result<()> {
        Ok(())
    }
    async fn get(&self, _index: u32) -> Result<Option<PageData<Self::Vec>>> {
        Ok(None)
    }
    fn size(&self) -> ByteSize {
        ByteSize::b(u64::MAX)
    }

    fn page_size(&self) -> usize {
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

        let page = cache.get_mut(20).await;
        //this block does not exist in the cache file yet.
        // the NullStore is HUGE in size, so while the cache size is only 10kib
        // blocks can be retrieved behind the cache size but as long as they
        // are
        assert!(page.is_ok());

        let mut page = page.unwrap();
        assert!(!page.header().flag(Flags::Dirty));
        assert!(page.header().flag(Flags::Occupied));
        assert!(page.data().iter().all(|f| *f == 0));
        assert_eq!(page.data().len(), 1024);

        page.data_mut().fill(10);
        page.header_mut().set(Flags::Dirty, true);

        let page = cache.get(20).await;
        assert!(page.is_ok());

        let page = page.unwrap();

        assert!(page.header().flag(Flags::Dirty));
        assert!(page.header().flag(Flags::Occupied));
        assert!(page.data().iter().all(|f| *f == 10));
        assert_eq!(page.data().len(), 1024);
    }

    #[tokio::test]
    async fn test_cache_reload() {
        const PATH: &str = "/tmp/cache.reload.test";
        // start from clean slate
        let _ = std::fs::remove_file(PATH);

        let mut cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        let page = cache.get_mut(20).await;
        //this block does not exist in the cache file yet.
        // the NullStore is HUGE in size, so while the cache size is only 10kib
        // blocks can be retrieved behind the cache size but as long as they
        // are
        assert!(page.is_ok());

        let mut page = page.unwrap();
        assert!(!page.header().flag(Flags::Dirty));
        assert!(page.header().flag(Flags::Occupied));
        assert!(page.data().iter().all(|f| *f == 0));
        assert_eq!(page.data().len(), 1024);

        page.data_mut().fill(10);
        page.header_mut().set(Flags::Dirty, true);

        // drop cache
        drop(cache);

        let mut cache = Cache::new(NullStore, PATH, ByteSize::kib(10), ByteSize::kib(1)).unwrap();

        // block 0 was not here before just to make sure
        let page = cache.get(0).await;
        assert!(page.is_ok());

        let page = page.unwrap();
        assert!(!page.header().flag(Flags::Dirty));
        assert!(page.header().flag(Flags::Occupied));
        assert!(page.data().iter().all(|f| *f == 0));
        assert_eq!(page.data().len(), 1024);

        // this is from before the drop it should still be fine
        let page = cache.get(20).await;
        assert!(page.is_ok());

        let page = page.unwrap();

        assert!(page.header().flag(Flags::Dirty));
        assert!(page.header().flag(Flags::Occupied));
        assert!(page.data().iter().all(|f| *f == 10));
        assert_eq!(page.data().len(), 1024);
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

        assert_eq!(cache.page_count(), 5);

        let page = cache.get_mut(9).await;
        assert!(page.is_ok());
        let mut page = page.unwrap();
        assert_eq!(page.address(), 0); // sanity check
                                       // we need this otherwise cache won't evict it
        page.header_mut().set(Flags::Dirty, true);
        // fill it with something
        page.data_mut().fill_with(|| 7);
        page.update_crc();

        assert_eq!(cache.occupied(), 1);

        // cache can hold only 5 blocks. It already now holds 1 (block 9). If we get 5 more, block 9 should be evicted
        assert_eq!(cache.get(0).await.unwrap().address(), 1);
        assert_eq!(cache.get(1).await.unwrap().address(), 2);
        assert_eq!(cache.get(2).await.unwrap().address(), 3);
        assert_eq!(cache.get(3).await.unwrap().address(), 4);
        assert_eq!(cache.get(4).await.unwrap().address(), 0);
        assert_eq!(cache.get(5).await.unwrap().address(), 1);

        assert_eq!(cache.occupied(), 5);

        let mem = cache.inner();

        // while we should except 2 blocks more evicted because we
        // have pushed total of 7 blocks, but only block 9 was dirty
        // hence block 0 (the last to be evicted) is in fact not dirty
        assert_eq!(mem.mem.len(), 1);

        assert!(mem.mem.get(&9).is_some());

        // open cache again with the same memory
        let mut cache = Cache::new(mem, PATH, ByteSize::kib(5), ByteSize::kib(1)).unwrap();

        let page = cache.get(9).await;
        assert!(page.is_ok());
        let page = page.unwrap();
        // sanity check
        assert_eq!(page.address(), 0);
        // the block here was retrieved from map, so it shouldn't be dirty
        assert!(!page.header().flag(Flags::Dirty));
        assert!(page.data().iter().all(|v| *v == 7));
        assert!(page.is_crc_ok());

        assert_eq!(cache.occupied(), 5);
    }
}
