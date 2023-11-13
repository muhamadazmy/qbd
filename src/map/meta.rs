use binary_layout::prelude::*;

const MAGIC: u32 = 0x617a6d79;
pub const VERSION: u32 = 1;

use crate::{Error, Result};

define_layout!(meta, BigEndian, {
    magic: u32,
    version: u32,
    page_size: u64,
    data_size: u64,
});

/// full size of the meta object
pub const SIZE: usize = 24;

/// Meta object
pub struct Meta {
    pub version: u32,
    pub page_size: u64,
    pub data_size: u64,
}

impl Meta {
    pub fn write(&self, buf: &mut [u8]) -> Result<()> {
        if buf.len() != meta::SIZE.unwrap() {
            return Err(Error::InvalidMetaSize);
        }

        let mut view = meta::View::new(buf);
        view.magic_mut().write(MAGIC);
        view.version_mut().write(self.version);
        view.page_size_mut().write(self.page_size);
        view.data_size_mut().write(self.data_size);

        Ok(())
    }

    pub fn load(buf: &[u8]) -> Result<Self> {
        if buf.len() != meta::SIZE.unwrap() {
            return Err(Error::InvalidMetaSize);
        }

        let view = meta::View::new(buf);

        if view.magic().read() != MAGIC {
            return Err(Error::InvalidMetaMagic);
        }

        Ok(Meta {
            version: VERSION,
            page_size: view.page_size().read(),
            data_size: view.data_size().read(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn size() {
        assert!(matches!(Some(SIZE), meta::SIZE));
    }
}
