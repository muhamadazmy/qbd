//use std::io::Error;
use std::io::Error as IoError;
use std::ops::Deref;

mod file;
pub mod policy;
mod sled_store;

use crate::{Error, Result};
use bytesize::ByteSize;
pub use file::FileStore;
pub use sled_store::SledStore;

/// Data is like built in Cow but read only
/// this allow stores to return data with no copy
/// if possible
pub enum Page<'a, T>
where
    T: Deref<Target = [u8]>,
{
    Owned(T),
    Borrowed(&'a [u8]),
}

impl<'a, T> Deref for Page<'a, T>
where
    T: Deref<Target = [u8]>,
{
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(v) => v,
        }
    }
}

#[async_trait::async_trait]
pub trait Store: Send + Sync + 'static {
    type Vec: Deref<Target = [u8]>;

    /// set a page it the store
    async fn set(&mut self, index: u32, page: &[u8]) -> Result<()>;

    /// get a page from the store
    async fn get(&self, index: u32) -> Result<Option<Page<Self::Vec>>>;

    /// size of the store
    fn size(&self) -> ByteSize;

    /// size of the page
    fn page_size(&self) -> usize;
}

#[cfg(test)]
pub use test::InMemory;

#[cfg(test)]
mod test {

    use super::*;
    use std::collections::HashMap;

    pub struct InMemory {
        pub mem: HashMap<u32, Vec<u8>>,
        cap: usize,
    }

    impl InMemory {
        pub fn new(cap: usize) -> Self {
            Self {
                mem: HashMap::with_capacity(cap),
                cap,
            }
        }
    }
    #[async_trait::async_trait]
    impl Store for InMemory {
        type Vec = Vec<u8>;
        async fn set(&mut self, index: u32, page: &[u8]) -> Result<()> {
            self.mem.insert(index, Vec::from(page));
            Ok(())
        }

        async fn get(&self, index: u32) -> Result<Option<Page<Self::Vec>>> {
            Ok(self.mem.get(&index).map(|d| Page::Borrowed(&d)))
        }

        fn size(&self) -> ByteSize {
            ByteSize((self.cap * self.page_size()) as u64)
        }

        fn page_size(&self) -> usize {
            1024
        }
    }
}
