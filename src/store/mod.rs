//use std::io::Error;
use std::io::Error as IoError;
use std::ops::Deref;

mod map;
use crate::{Error, Result};
use bytesize::ByteSize;
pub use map::MapStore;
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
}
