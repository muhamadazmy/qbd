use std::path::Path;

use bytesize::ByteSize;

//use crate::map::{BlockMap, Flags};

use super::*;
use sled::Db;

/// MapStore implements a store on a mmap file.

pub struct SledStore {
    db: Db,
    size: ByteSize,
    bs: ByteSize,
    bc: u32,
}

impl SledStore {
    pub fn new<P: AsRef<Path>>(path: P, size: ByteSize, bs: ByteSize) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(Self {
            db,
            size,
            bs,
            bc: (size.0 / bs.0) as u32,
        })
    }
}

#[async_trait::async_trait]
impl Store for SledStore {
    type Vec = sled::IVec;

    async fn set(&mut self, index: u32, data: &[u8]) -> Result<()> {
        if index >= self.bc {
            return Err(Error::PageIndexOutOfRange);
        }
        if data.len() != self.bs.0 as usize {
            return Err(Error::InvalidPageSize);
        }

        self.db.insert(index.to_be_bytes(), data)?;
        Ok(())
    }

    async fn get(&self, index: u32) -> Result<Option<Data<Self::Vec>>> {
        if index >= self.bc {
            return Err(Error::PageIndexOutOfRange);
        }
        let data = self.db.get(index.to_be_bytes())?.map(Data::Owned);

        Ok(data)
    }

    fn size(&self) -> ByteSize {
        self.size
    }

    fn block_size(&self) -> usize {
        self.bs.0 as usize
    }
}
