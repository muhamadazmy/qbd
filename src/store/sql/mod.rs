use super::{Page, Store};
use crate::{Error, Result};
use anyhow::Context;
use bytesize::ByteSize;
use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    SqlitePool,
};

static SCHEMA: &str = include_str!("schema.sql");

pub struct SqliteStore {
    pool: SqlitePool,
    size: ByteSize,
    page_size: ByteSize,
}

impl SqliteStore {
    pub async fn new<P: AsRef<Path>>(path: P, size: ByteSize, page_size: ByteSize) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete)
            .filename(path.as_ref());

        let pool = SqlitePool::connect_with(opts)
            .await
            .context("open sqlite database")?;

        sqlx::query(SCHEMA)
            .execute(&pool)
            .await
            .context("create sqlite schema")?;

        Ok(Self {
            pool,
            size,
            page_size,
        })
    }
}

#[async_trait::async_trait]
impl Store for SqliteStore {
    /// set a page it the store
    async fn set(&mut self, index: u32, page: &[u8]) -> Result<()> {
        if page.len() != self.page_size() {
            return Err(Error::InvalidPageSize);
        }

        sqlx::query("insert or replace into kv (key, value) values (?, ?);")
            .bind(index)
            .bind(page)
            .execute(&self.pool)
            .await
            .context("inserting recording in database")?;

        Ok(())
    }

    /// get a page from the store
    async fn get(&self, index: u32) -> Result<Option<Page>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as("select value from kv where key = ?;")
            .bind(index)
            .fetch_optional(&self.pool)
            .await
            .context("query failed")?;

        Ok(row.map(|(v,)| Page::Owned(v)))
    }

    /// size of the store
    fn size(&self) -> ByteSize {
        self.size
    }

    /// size of the page
    fn page_size(&self) -> usize {
        self.page_size.0 as usize
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;

    use bytesize::ByteSize;

    use super::{SqliteStore, Store};

    #[tokio::test]
    async fn store() {
        let page = "hello world";
        let mut store = SqliteStore::new(
            "/tmp/store.sql",
            ByteSize::gib(10),
            ByteSize(page.len() as u64),
        )
        .await
        .unwrap();

        store.set(10, page.as_bytes()).await.unwrap();

        let value = store.get(10).await.unwrap();
        assert!(value.is_some());
        let value = value.unwrap();
        assert_eq!(value.deref(), page.as_bytes());
    }
}
