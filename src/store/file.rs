use std::path::Path;

use bytesize::ByteSize;

use crate::map::{Flags, PageMap};

use super::*;

/// persisted storage using BlockMap
pub struct FileStore {
    map: PageMap,
    size: ByteSize,
}

impl FileStore {
    pub fn new<P: AsRef<Path>>(path: P, size: ByteSize, page_size: ByteSize) -> Result<Self> {
        Ok(Self {
            map: PageMap::new(path, size, page_size).map_err(IoError::from)?,
            size,
        })
    }
}

#[async_trait::async_trait]
impl Store for FileStore {
    type Vec = Vec<u8>;

    async fn set(&mut self, index: u32, data: &[u8]) -> Result<()> {
        if data.len() != self.map.page_size() {
            return Err(Error::InvalidPageSize);
        }

        let mut block = self.map.at_mut(index as usize);
        block.data_mut().copy_from_slice(data);
        block
            .header_mut()
            .set_page(index)
            .set(Flags::Occupied, true);
        block.update_crc();

        // this flushes the block immediately, may
        // be for performance improvements we shouldn't
        // do that or use async way
        self.map.flush_page(index as usize)
    }

    async fn get(&self, index: u32) -> Result<Option<Data<Self::Vec>>> {
        // we access the map directly to avoid a borrow problem
        let header = self.map.header_at(index as usize);
        if !header.flag(Flags::Occupied) {
            return Ok(None);
        }

        let data = self.map.data_at(index as usize);

        Ok(Some(Data::Borrowed(data)))
    }

    fn size(&self) -> ByteSize {
        self.size
    }

    fn block_size(&self) -> usize {
        self.map.page_size()
    }
}
