use crate::{cache::Cache, map::Header};
use std::io;

struct Device {
    cache: Cache,
}

impl Device {
    pub fn new(cache: Cache) -> Self {
        Self { cache }
    }

    /// we can only map blocks index that fits in a u32.
    /// this is because
    pub fn block_of(&self, offset: u64) -> io::Result<u32> {
        let block = offset / self.cache.block_size();

        u32::try_from(block).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
    }
}

#[async_trait::async_trait(?Send)]
impl nbd_async::BlockDevice for Device {
    async fn read(&mut self, offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        // find the block

        let mut index = self.block_of(offset)?;
        let mut inner_offset = (offset % self.cache.block_size()) as usize;

        loop {
            let block = self.cache.get(index);
            // TODO: instead of initializing an empty block we should fetch from
            // cold storage. this is temporary
            let block = match block {
                Some(block) => block,
                None => self
                    .cache
                    .put(Header::new(index), None, |_, _| unimplemented!())?
                    .into(),
            };

            let source = &block.data()[inner_offset..];
            let to_copy = std::cmp::min(source.len(), buf.len());
            buf[..to_copy].copy_from_slice(&source[..to_copy]);
            buf = &mut buf[to_copy..];
            if buf.len() == 0 {
                break;
            }
            index += 1;
            inner_offset = 0;
        }

        Ok(())
    }
    /// Write a block of data at offset.
    async fn write(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        // let offset = offset as usize;
        // self.map[offset..offset + buf.len()].copy_from_slice(buf);
        // Ok(())
        unimplemented!()
    }

    /// Flushes write buffers to the underlying storage medium
    async fn flush(&mut self) -> io::Result<()> {
        self.cache.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    #[tokio::test]
    async fn read() {}
}
