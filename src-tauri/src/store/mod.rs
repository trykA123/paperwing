pub mod commits;
mod error;
pub mod listings;
mod migrations;
mod open;
mod readers;
#[cfg_attr(not(test), allow(dead_code))]
pub mod refs;
#[cfg_attr(not(test), allow(dead_code))]
pub mod search;
mod size;
mod writer;

#[cfg(test)]
mod tests;

pub use commits::CommitRow;
pub use error::Error;

use migrations::{Migration, MIGRATIONS};
use readers::Readers;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use writer::Writer;

const FILE_NAME: &str = "skein-store.sqlite3";
const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const READER_COUNT: usize = 3;

pub struct Options {
    pub max_bytes: u64,
    pub readers: usize,
    pub migrations: &'static [Migration],
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            readers: READER_COUNT,
            migrations: MIGRATIONS,
        }
    }
}

struct Inner {
    writer: Writer,
    readers: Readers,
    recovered_from: Option<PathBuf>,
}

#[derive(Clone)]
pub struct Store {
    inner: Option<Arc<Inner>>,
}

pub fn seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Store {
    pub fn open(path: &Path, options: &Options) -> Result<Self, Error> {
        let opened = open::open_writer(path, options.migrations)?;
        let readers = Readers::open(path, options.readers)?;
        let writer = Writer::spawn(opened.connection, options.max_bytes)?;
        Ok(Self {
            inner: Some(Arc::new(Inner {
                writer,
                readers,
                recovered_from: opened.recovered_from,
            })),
        })
    }

    pub fn open_in(directory: &Path) -> Self {
        match Self::open(&directory.join(FILE_NAME), &Options::default()) {
            Ok(store) => {
                if let Some(kept) = store.recovered_from() {
                    eprintln!(
                        "Local store reset; the old file is kept at {}",
                        kept.display()
                    );
                }
                store
            }
            Err(error) => {
                eprintln!("Local store disabled: {error}");
                Self::disabled()
            }
        }
    }

    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn recovered_from(&self) -> Option<&Path> {
        self.inner.as_ref()?.recovered_from.as_deref()
    }

    fn inner(&self) -> Result<&Inner, Error> {
        self.inner.as_deref().ok_or(Error::Unavailable)
    }

    pub fn write_blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        self.inner()?
            .writer
            .submit(work)
            .blocking_recv()
            .map_err(|_| Error::Unavailable)?
    }

    pub fn post(&self, work: impl FnOnce(&mut Connection) -> Result<(), Error> + Send + 'static) {
        let Ok(inner) = self.inner() else { return };
        drop(inner.writer.submit(move |connection| {
            let result = work(connection);
            if let Err(error) = &result {
                eprintln!("Local store write failed: {error}");
            }
            result
        }));
    }

    pub fn read_blocking<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.inner()?.readers.with(work)
    }

    pub fn remove_source(&self, source_id: &str) {
        let source_id = source_id.to_string();
        self.post(move |connection| {
            let transaction = connection.transaction()?;
            listings::remove(&transaction, &source_id)?;
            commits::remove_source(&transaction, &source_id)?;
            Ok(transaction.commit()?)
        });
    }
}
