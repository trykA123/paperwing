pub mod commits;
mod error;
mod legacy;
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
    sync::{Arc, OnceLock},
};
use tokio::sync::oneshot;
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
    path: PathBuf,
}

#[derive(Clone)]
pub struct Store {
    slot: Arc<OnceLock<Option<Arc<Inner>>>>,
}

pub fn seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Store {
    #[cfg(test)]
    pub fn open(path: &Path, options: &Options) -> Result<Self, Error> {
        let store = Self::pending();
        store.fill(Self::build(path, options));
        Ok(store)
    }

    fn build(path: &Path, options: &Options) -> Result<Arc<Inner>, Error> {
        let opened = open::open_writer(path, options.migrations)?;
        let readers = Readers::open(path, options.readers)?;
        let writer = Writer::spawn(opened.connection, options.max_bytes)?;
        Ok(Arc::new(Inner {
            writer,
            readers,
            recovered_from: opened.recovered_from,
            path: path.to_path_buf(),
        }))
    }

    pub fn pending() -> Self {
        Self {
            slot: Arc::new(OnceLock::new()),
        }
    }

    #[cfg(test)]
    pub fn disabled() -> Self {
        let store = Self::pending();
        store.fill(Err(Error::Unavailable));
        store
    }

    pub fn start(data: PathBuf, cache: Option<PathBuf>) -> Self {
        let store = Self::pending();
        let opening = store.clone();
        let spawned = std::thread::Builder::new()
            .name("store-open".into())
            .spawn(move || {
                opening.fill(Self::build(&data.join(FILE_NAME), &Options::default()));
                if let Some(cache) = cache {
                    legacy::remove_listing_files(&cache);
                }
            });
        if let Err(error) = spawned {
            eprintln!("Local store disabled: {error}");
            store.fill(Err(Error::Unavailable));
        }
        store
    }

    fn fill(&self, result: Result<Arc<Inner>, Error>) {
        let inner = match result {
            Ok(inner) => {
                if let Some(kept) = inner.recovered_from.as_deref() {
                    eprintln!(
                        "Local store reset; the old file is kept at {}",
                        kept.display()
                    );
                }
                Some(inner)
            }
            Err(error) => {
                eprintln!("Local store disabled: {error}");
                None
            }
        };
        let _ = self.slot.set(inner);
    }

    #[cfg(test)]
    pub fn is_ready(&self) -> bool {
        matches!(self.slot.get(), Some(Some(_)))
    }

    pub fn mark_clean(&self) {
        if let Ok(inner) = self.inner() {
            open::mark_clean(&inner.path);
        }
    }

    #[cfg(test)]
    pub fn recovered_from(&self) -> Option<&Path> {
        self.inner().ok()?.recovered_from.as_deref()
    }

    fn inner(&self) -> Result<&Inner, Error> {
        self.slot
            .get()
            .and_then(Option::as_deref)
            .ok_or(Error::Unavailable)
    }

    #[cfg(test)]
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

    pub fn enqueue<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    ) -> Result<oneshot::Receiver<Result<T, Error>>, Error> {
        Ok(self.inner()?.writer.submit(work))
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
