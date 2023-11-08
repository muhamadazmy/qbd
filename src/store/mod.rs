//use std::io::Error;
use std::io::Error as IoError;
use std::ops::Deref;

mod file;
mod sled_store;

use crate::{Error, Result};
use bytesize::ByteSize;
pub use file::FileStore;
pub use sled_store::SledStore;

/// Data is like built in Cow but read only
/// this allow stores to return data with no copy
/// if possible
pub enum Data<'a, T>
where
    T: Deref<Target = [u8]>,
{
    Owned(T),
    Borrowed(&'a [u8]),
}

impl<'a, T> Deref for Data<'a, T>
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

    async fn set(&mut self, index: u32, block: &[u8]) -> Result<()>;
    async fn get(&self, index: u32) -> Result<Option<Data<Self::Vec>>>;
    fn size(&self) -> ByteSize;
    fn block_size(&self) -> usize;
}

/// ConcatStore takes multiple stores and makes them
/// act like a single big store where size = sum(sizes)
pub struct ConcatStore<S> {
    parts: Vec<S>,
    bs: usize,
}

impl<S> ConcatStore<S>
where
    S: Store,
{
    pub fn new(parts: Vec<S>) -> Result<Self> {
        if parts.is_empty() {
            return Err(Error::ZeroSize);
        }

        let bs = parts[0].block_size();
        if !parts.iter().all(|f| f.block_size() == bs) {
            return Err(Error::InvalidBlockSize);
        }

        Ok(Self { parts, bs })
    }
}

#[async_trait::async_trait]
impl<S> Store for ConcatStore<S>
where
    S: Store,
{
    type Vec = S::Vec;

    async fn set(&mut self, index: u32, block: &[u8]) -> Result<()> {
        let mut index = index as usize;
        for store in self.parts.iter_mut() {
            let bc = store.size().0 as usize / self.bs;
            if index < bc {
                return store.set(index as u32, block).await;
            }

            index -= bc;
        }

        Err(Error::BlockIndexOutOfRange)
    }

    async fn get(&self, index: u32) -> Result<Option<Data<Self::Vec>>> {
        let mut index = index as usize;
        for store in self.parts.iter() {
            let bc = store.size().0 as usize / self.bs;
            if index < bc {
                return store.get(index as u32).await;
            }

            index -= bc;
        }

        Err(Error::BlockIndexOutOfRange)
    }

    fn size(&self) -> ByteSize {
        self.parts.iter().fold(ByteSize(0), |t, i| t + i.size())
    }

    fn block_size(&self) -> usize {
        self.bs
    }
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
        async fn set(&mut self, index: u32, block: &[u8]) -> Result<()> {
            self.mem.insert(index, Vec::from(block));
            Ok(())
        }

        async fn get(&self, index: u32) -> Result<Option<Data<Self::Vec>>> {
            Ok(self.mem.get(&index).map(|d| Data::Borrowed(&d)))
        }

        fn size(&self) -> ByteSize {
            ByteSize((self.cap * self.block_size()) as u64)
        }

        fn block_size(&self) -> usize {
            1024
        }
    }

    #[tokio::test]
    async fn test_concat() {
        let mut store = ConcatStore::new(vec![InMemory::new(10), InMemory::new(10)]).unwrap();
        assert_eq!(store.block_size(), 1024);
        assert_eq!(store.size(), ByteSize(20 * 1024)); // 20 blocks each of 1024 bytes

        let b0 = store.get(0).await.unwrap();
        let b10 = store.get(10).await.unwrap();
        let b19 = store.get(19).await.unwrap();

        assert!(store.get(20).await.is_err());

        assert!(b0.is_none());
        assert!(b10.is_none());
        assert!(b19.is_none());

        let data: [u8; 1024] = [70; 1024];
        assert!(store.set(10, &data).await.is_ok());

        let b10 = store.get(10).await.unwrap();
        assert!(b10.is_some());
        let b10 = b10.unwrap();
        b10.iter().all(|f| *f == 70);

        let mem = &store.parts[1].mem;
        assert_eq!(mem.len(), 1);
        // because the concat store recalculate the offsets
        // this then should be at index 0
        assert!(mem.get(&0).is_some());
    }
}
