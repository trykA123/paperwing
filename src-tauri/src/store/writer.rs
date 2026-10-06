use super::error::Error;
use super::size;
use rusqlite::Connection;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

pub(super) struct Writer {
    sender: Mutex<Option<mpsc::UnboundedSender<Job>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Writer {
    pub fn spawn(mut connection: Connection, max_bytes: u64) -> Result<Self, Error> {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("store-writer".into())
            .spawn(move || {
                while let Some(job) = receiver.blocking_recv() {
                    if catch_unwind(AssertUnwindSafe(|| job(&mut connection))).is_err() {
                        eprintln!("Local store write panicked; the write was dropped");
                    }
                    if let Err(error) = size::enforce(&mut connection, max_bytes) {
                        eprintln!("Local store pruning failed: {error}");
                    }
                }
            })?;
        Ok(Self {
            sender: Mutex::new(Some(sender)),
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn submit<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    ) -> oneshot::Receiver<Result<T, Error>> {
        let (reply, receiver) = oneshot::channel();
        let job: Job = Box::new(move |connection| {
            let _ = reply.send(work(connection));
        });
        if let Some(sender) = self
            .sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = sender.send(job);
        }
        receiver
    }

    pub fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(thread) = thread {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.close();
    }
}
