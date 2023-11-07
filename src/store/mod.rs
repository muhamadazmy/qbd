//use std::io::Error;
use std::io::Error as IoError;
use std::ops::Deref;
mod map;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid block size")]
    InvalidBlockSize,

    #[error("io error: {0}")]
    IO(#[from] IoError),
}

impl From<Error> for std::io::Error {
    fn from(value: Error) -> Self {
        use std::io::ErrorKind;

        // TODO: possible different error kind
        match value {
            Error::IO(err) => err,
            _ => IoError::new(ErrorKind::InvalidInput, value),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

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
            Self::Owned(v) => &v,
            Self::Borrowed(v) => v,
        }
    }
}

#[async_trait::async_trait]
pub trait Store: Send + Sync + 'static {
    async fn set(&mut self, index: u32, block: &[u8]) -> Result<()>;
    async fn get(&self, index: u32) -> Result<Option<Data>>;
    fn size(&self) -> usize;
}
