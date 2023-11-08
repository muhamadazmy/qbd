//use std::io::Error;
use std::io::Error as IoError;
use std::ops::Deref;

mod file;
use crate::{Error, Result};
use bytesize::ByteSize;
pub use file::FileStore;
/// Data is like built in Cow but read only
/// this allow stores to return data with no copy
/// if possible
pub enum Data<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a [u8]),
}

impl<'a> Deref for Data<'a> {
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
    async fn set(&mut self, index: u32, block: &[u8]) -> Result<()>;
    async fn get(&self, index: u32) -> Result<Option<Data>>;
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
    async fn set(&mut self, index: u32, block: &[u8]) -> Result<()> {
        let mut index = index as usize;
        for store in self.parts.iter_mut() {
            let bc = store.size().0 as usize / store.block_size();
            if index < bc {
                return store.set(index as u32, block).await;
            }

            index -= bc;
        }

        Err(Error::BlockIndexOutOfRange)
    }

    async fn get(&self, index: u32) -> Result<Option<Data>> {
        let mut index = index as usize;
        for store in self.parts.iter() {
            let bc = store.size().0 as usize / store.block_size();
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
