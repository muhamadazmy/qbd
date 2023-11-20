//! policy module implements a set of special stores types that does
//! not store the data on its own but uses other stores and applies some
//! policy on it.
//!
//! for example a ConcatStore appends 2 or more stores together so that
//! they appear as a bigger single store.
mod concat;
mod mirror;
mod strip;

use bytesize::ByteSize;
pub use concat::ConcatPolicy;
pub use mirror::MirrorPolicy;
pub use strip::StripPolicy;

use super::{Page, Store};
use crate::Result;

pub enum Policy<S>
where
    S: Store,
{
    Concat(ConcatPolicy<S>),
    Strip(StripPolicy<S>),
    Mirror(MirrorPolicy),
}

impl<S> Policy<S>
where
    S: Store,
{
    /// build a new concat policy from parts
    pub fn concat(parts: Vec<S>) -> Result<Self> {
        Ok(Self::Concat(ConcatPolicy::new(parts)?))
    }

    /// build a new strip policy from parts
    pub fn strip(parts: Vec<S>) -> Result<Self> {
        Ok(Self::Strip(StripPolicy::new(parts)?))
    }

    pub fn mirror(parts: Vec<S>) -> Result<Self> {
        Ok(Self::Mirror(MirrorPolicy::new(parts)?))
    }
}

#[async_trait::async_trait]
impl<S> Store for Policy<S>
where
    S: Store,
{
    /// set a page it the store
    async fn set(&mut self, index: u32, page: &[u8]) -> Result<()> {
        match self {
            Self::Concat(inner) => inner.set(index, page).await,
            Self::Strip(inner) => inner.set(index, page).await,
            Self::Mirror(inner) => inner.set(index, page).await,
        }
    }

    /// get a page from the store
    async fn get(&self, index: u32) -> Result<Option<Page>> {
        match self {
            Self::Concat(inner) => inner.get(index).await,
            Self::Strip(inner) => inner.get(index).await,
            Self::Mirror(inner) => inner.get(index).await,
        }
    }

    /// size of the store
    fn size(&self) -> ByteSize {
        match self {
            Self::Concat(inner) => inner.size(),
            Self::Strip(inner) => inner.size(),
            Self::Mirror(inner) => inner.size(),
        }
    }

    /// size of the page
    fn page_size(&self) -> usize {
        match self {
            Self::Concat(inner) => inner.page_size(),
            Self::Strip(inner) => inner.page_size(),
            Self::Mirror(inner) => inner.page_size(),
        }
    }
}
