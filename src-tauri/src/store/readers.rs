use super::error::Error;
use super::open;
use rusqlite::Connection;
use std::{
    path::Path,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
};

pub(super) struct Readers {
    idle: Mutex<Vec<Connection>>,
    returned: Condvar,
}

impl Readers {
    pub fn open(path: &Path, size: usize) -> Result<Self, Error> {
        let connections = (0..size.max(1))
            .map(|_| open::open_reader(path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            idle: Mutex::new(connections),
            returned: Condvar::new(),
        })
    }

    pub fn with<T>(&self, work: impl FnOnce(&Connection) -> Result<T, Error>) -> Result<T, Error> {
        let lease = self.lease();
        let transaction = lease.connection().unchecked_transaction()?;
        work(&transaction)
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Connection>> {
        self.idle.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lease(&self) -> Lease<'_> {
        let mut idle = self.lock();
        loop {
            if let Some(connection) = idle.pop() {
                return Lease {
                    readers: self,
                    connection: Some(connection),
                };
            }
            idle = self
                .returned
                .wait(idle)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

struct Lease<'a> {
    readers: &'a Readers,
    connection: Option<Connection>,
}

impl Lease<'_> {
    fn connection(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("a lease holds its connection until it is dropped")
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.readers.lock().push(connection);
            self.readers.returned.notify_one();
        }
    }
}
