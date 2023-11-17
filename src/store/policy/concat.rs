use crate::store::{Page, Store};
use crate::{Error, Result};
use bytesize::ByteSize;

/// ConcatStore takes multiple stores and makes them
/// act like a single big store where size = sum(sizes)
pub struct ConcatPolicy<S> {
    parts: Vec<S>,
    ps: usize,
}

impl<S> ConcatPolicy<S>
where
    S: Store,
{
    pub fn new(parts: Vec<S>) -> Result<Self> {
        if parts.is_empty() {
            return Err(Error::ZeroSize);
        }

        let ps = parts[0].page_size();
        if !parts.iter().all(|f| f.page_size() == ps) {
            return Err(Error::InvalidPageSize);
        }

        Ok(Self { parts, ps })
    }
}

#[async_trait::async_trait]
impl<S> Store for ConcatPolicy<S>
where
    S: Store,
{
    type Vec = S::Vec;

    async fn set(&mut self, index: u32, page: &[u8]) -> Result<()> {
        let mut index = index as usize;
        for store in self.parts.iter_mut() {
            let bc = store.size().0 as usize / self.ps;
            if index < bc {
                return store.set(index as u32, page).await;
            }

            index -= bc;
        }

        Err(Error::PageIndexOutOfRange)
    }

    async fn get(&self, index: u32) -> Result<Option<Page<Self::Vec>>> {
        let mut index = index as usize;
        for store in self.parts.iter() {
            let bc = store.size().0 as usize / self.ps;
            if index < bc {
                return store.get(index as u32).await;
            }

            index -= bc;
        }

        Err(Error::PageIndexOutOfRange)
    }

    fn size(&self) -> ByteSize {
        self.parts.iter().fold(ByteSize(0), |t, i| t + i.size())
    }

    fn page_size(&self) -> usize {
        self.ps
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::store::InMemory;

    #[tokio::test]
    async fn test_concat() {
        let mut store = ConcatPolicy::new(vec![InMemory::new(10), InMemory::new(10)]).unwrap();
        assert_eq!(store.page_size(), 1024);
        assert_eq!(store.size(), ByteSize(20 * 1024)); // 20 blocks each of 1024 bytes

        let b0 = store.get(0).await.unwrap();
        let b10 = store.get(10).await.unwrap();
        let b19 = store.get(19).await.unwrap();

        assert!(store.get(20).await.is_err());

        assert!(b0.is_none());
        assert!(b10.is_none());
        assert!(b19.is_none());

        let data: [u8; 1024] = [70; 1024];
        assert!(store.set(10, &data).await.is_ok());

        let b10 = store.get(10).await.unwrap();
        assert!(b10.is_some());
        let b10 = b10.unwrap();
        b10.iter().all(|f| *f == 70);

        let mem = &store.parts[1].mem;
        assert_eq!(mem.len(), 1);
        // because the concat store recalculate the offsets
        // this then should be at index 0
        assert!(mem.get(&0).is_some());
    }
}
