use crate::store::{Page, Store};
use crate::{Error, PolicyError, Result};
use bytesize::ByteSize;

/// StripPolicy takes multiple stores and makes them
/// act like a single big store where size = sum(sizes)
/// the difference between concat store is that here
/// the blocks is stripped over the multiple stores like
/// raid0
///
/// WARNING: when using stripping it's not possible to later
/// add another store to the array otherwise all offsets and
/// locations will be wrong.
pub struct StripPolicy<S> {
    parts: Vec<S>,
    bs: usize,
    size: ByteSize,
}

impl<S> StripPolicy<S>
where
    S: Store,
{
    pub fn new(parts: Vec<S>) -> Result<Self> {
        if parts.is_empty() {
            return Err(Error::ZeroSize);
        }
        let size = parts[0].size();
        if !parts.iter().all(|f| f.size() == size) {
            return Err(PolicyError::StoresNotSameSize.into());
        }

        let bs = parts[0].page_size();
        if !parts.iter().all(|f| f.page_size() == bs) {
            return Err(Error::InvalidPageSize);
        }

        let total_size = size.0 * parts.len() as u64;
        Ok(Self {
            parts,
            bs,
            size: ByteSize(total_size),
        })
    }
}

#[async_trait::async_trait]
impl<S> Store for StripPolicy<S>
where
    S: Store,
{
    async fn set(&mut self, index: u32, page: &[u8]) -> Result<()> {
        if index as u64 >= self.size.0 {
            return Err(Error::PageIndexOutOfRange);
        }

        let outer = index as usize % self.parts.len();
        let inner = index as usize / self.parts.len();

        self.parts[outer].set(inner as u32, page).await
    }

    async fn get(&self, index: u32) -> Result<Option<Page>> {
        if index as u64 >= self.size.0 {
            return Err(Error::PageIndexOutOfRange);
        }

        let outer = index as usize % self.parts.len();
        let inner = index as usize / self.parts.len();

        self.parts[outer].get(inner as u32).await
    }

    fn size(&self) -> ByteSize {
        self.size
    }

    fn page_size(&self) -> usize {
        self.bs
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;

    use crate::store::InMemory;

    use super::{Store, StripPolicy};

    #[tokio::test]
    async fn stripping() {
        let parts = vec![InMemory::new(10), InMemory::new(10)];

        let mut stripping = StripPolicy::new(parts).unwrap();

        stripping.set(0, "hello".as_bytes()).await.unwrap();
        stripping.set(1, "world".as_bytes()).await.unwrap();

        assert_eq!(
            stripping.parts[0].get(0).await.unwrap().unwrap().deref(),
            "hello".as_bytes()
        );

        assert_eq!(
            stripping.parts[1].get(0).await.unwrap().unwrap().deref(),
            "world".as_bytes()
        );
    }
}
