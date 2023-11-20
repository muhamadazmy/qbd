use crate::store::{Page, Store};
use crate::{Error, PolicyError, Result};
use anyhow::Context;
use bytesize::ByteSize;
use std::sync::Arc;
use tokio::task::JoinSet;

use tokio::sync::mpsc::Sender as Channel;
use tokio::sync::oneshot::Sender as OneShotSender;

enum Request {
    Set {
        index: u32,
        page: Arc<Vec<u8>>,
        reply_on: OneShotSender<Result<()>>,
    },
    Get {
        index: u32,
        reply_on: OneShotSender<Result<Option<Vec<u8>>>>,
    },
}

fn mirror<S: Store>(mut store: S) -> Channel<Request> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            match request {
                Request::Get { index, reply_on } => {
                    let result = store.get(index).await.map(|v| v.map(Vec::<u8>::from));

                    let _ = reply_on.send(result);
                }
                Request::Set {
                    index,
                    page,
                    reply_on,
                } => {
                    let result = store.set(index, &page).await;
                    let _ = reply_on.send(result);
                }
            }
        }
    });

    tx
}

/// MirrorPolicy takes multiple stores and makes them
/// act like a single mirrored stores where size = size of a single instance
/// on writing the data must be written to the 2 stores at the same time
/// on read, the data is retrieved from the first store that answers
pub struct MirrorPolicy {
    bs: usize,
    size: ByteSize,
    channels: Vec<Channel<Request>>,
}

impl MirrorPolicy {
    pub fn new<S: Store>(parts: Vec<S>) -> Result<Self> {
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

        let mut channels = vec![];
        for sub in parts {
            let ch = mirror(sub);
            channels.push(ch);
        }

        Ok(Self { bs, size, channels })
    }
}

#[async_trait::async_trait]
impl Store for MirrorPolicy {
    async fn set(&mut self, index: u32, page: &[u8]) -> Result<()> {
        if index as u64 >= self.size.0 {
            return Err(Error::PageIndexOutOfRange);
        }

        let page: Vec<u8> = page.into();
        let page = Arc::new(page);
        let mut set = JoinSet::new();
        for sub in self.channels.iter() {
            let (tx, rx) = tokio::sync::oneshot::channel();

            let request = Request::Set {
                index,
                page: Arc::clone(&page),
                reply_on: tx,
            };

            if sub.send(request).await.is_err() {
                log::error!("failed to send request to store");
                continue;
            }

            set.spawn(rx);
        }

        // the first Result is the join_next() result itself
        // inside that the result of `rx;await`
        // then the final result from the actual called operation
        while let Some(result) = set.join_next().await {
            // result is 3 layers of result since each can fail separated
            let result = result
                .context("joining set request")?
                .context("receive response from mirrored store")?;

            result?;
        }

        // all write operation succeeded
        Ok(())
    }

    async fn get(&self, index: u32) -> Result<Option<Page>> {
        if index as u64 >= self.size.0 {
            return Err(Error::PageIndexOutOfRange);
        }

        let mut set = JoinSet::new();
        for sub in self.channels.iter() {
            let (tx, rx) = tokio::sync::oneshot::channel();

            let request = Request::Get {
                index,
                reply_on: tx,
            };

            if sub.send(request).await.is_err() {
                log::error!("failed to send request to store");
                continue;
            }

            set.spawn(rx);
        }

        // the first Result is the join_next() result itself
        // inside that the result of `rx;await`
        // then the final result from the actual called operation
        while let Some(result) = set.join_next().await {
            // result is 3 layers of result since each can fail separated
            let result = result
                .context("joining set request")?
                .context("receive response from mirrored store")?;

            match result {
                Err(err) => {
                    log::error!("store return error: {:#}", err);
                    continue;
                }
                Ok(result) => {
                    return Ok(result.map(Page::Owned));
                }
            }
        }

        return Err(
            anyhow::anyhow!("all stores failed to answer the request, please check logs").into(),
        );
    }

    fn size(&self) -> ByteSize {
        self.size
    }

    fn page_size(&self) -> usize {
        self.bs
    }
}
