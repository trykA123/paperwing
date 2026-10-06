use super::error::Error;
use super::open;
use rusqlite::Connection;
use std::{
    path::Path,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
};

struct Pool {
    idle: Vec<Connection>,
    leased: usize,
    closed: bool,
}

pub(super) struct Readers {
    pool: Mutex<Pool>,
    changed: Condvar,
}

impl Readers {
    pub fn open(path: &Path, size: usize) -> Result<Self, Error> {
        let idle = (0..size.max(1))
            .map(|_| open::open_reader(path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            pool: Mutex::new(Pool {
                idle,
                leased: 0,
                closed: false,
            }),
            changed: Condvar::new(),
        })
    }

    pub fn with<T>(&self, work: impl FnOnce(&Connection) -> Result<T, Error>) -> Result<T, Error> {
        let lease = self.lease()?;
        let transaction = lease.connection().unchecked_transaction()?;
        work(&transaction)
    }

    pub fn close(&self) {
        let mut pool = self.lock();
        pool.closed = true;
        pool.idle.clear();
        while pool.leased > 0 {
            pool = self
                .changed
                .wait(pool)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Pool> {
        self.pool.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lease(&self) -> Result<Lease<'_>, Error> {
        let mut pool = self.lock();
        loop {
            if pool.closed {
                return Err(Error::Unavailable);
            }
            if let Some(connection) = pool.idle.pop() {
                pool.leased += 1;
                return Ok(Lease {
                    readers: self,
                    connection: Some(connection),
                });
            }
            pool = self
                .changed
                .wait(pool)
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
        let Some(connection) = self.connection.take() else {
            return;
        };
        let mut pool = self.readers.lock();
        pool.leased -= 1;
        if !pool.closed {
            pool.idle.push(connection);
        }
        drop(pool);
        self.readers.changed.notify_all();
    }
}
