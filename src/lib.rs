use std::io::{Error as IoError, ErrorKind};

pub mod cache;
//pub mod device;
pub mod map;
pub mod store;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("size cannot be zero")]
    ZeroSize,

    #[error("block size is too big")]
    BlockSizeTooBig,

    #[error("invalid block size")]
    InvalidBlockSize,

    #[error("block index out of range")]
    BlockIndexOutOfRange,

    // #[error("block count is too big")]
    // BlockCountTooBig,
    #[error("size must be multiple of block size")]
    SizeNotMultipleOfBlockSize,

    #[error("io error: {0}")]
    IO(#[from] IoError),
}

impl From<Error> for std::io::Error {
    fn from(value: Error) -> Self {
        // TODO: possible different error kind
        match value {
            Error::IO(err) => err,
            _ => IoError::new(ErrorKind::InvalidInput, value),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
