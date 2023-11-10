use std::{
    io::{Error as IoError, ErrorKind},
    path::PathBuf,
};

pub mod cache;
pub mod device;
pub mod map;
pub mod store;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("size cannot be zero")]
    ZeroSize,

    #[error("page size is too big")]
    PageSizeTooBig,

    #[error("page count is too big")]
    PageCountTooBig,

    #[error("invalid page size")]
    InvalidPageSize,

    #[error("page index out of range")]
    PageIndexOutOfRange,

    // #[error("block count is too big")]
    #[error("page size must be multiple of block size")]
    SizeNotMultipleOfPageSize,

    #[error("sled db error: {0}")]
    Sled(#[from] sled::Error),

    #[error("size change to file {0}")]
    SizeChanged(PathBuf),

    #[error("invalid meta size")]
    InvalidMetaSize,

    #[error("invalid meta magic")]
    InvalidMetaMagic,

    #[error("invalid meta version")]
    InvalidMetaVersion,

    #[error("invalid meta page size")]
    InvalidMetaPageSize,

    #[error("invalid meta data size")]
    InvalidMetaDataSize,

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
