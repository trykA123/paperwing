use super::error::Error;
use super::size;
use rusqlite::Connection;
use std::panic::{catch_unwind, AssertUnwindSafe};
use tokio::sync::{mpsc, oneshot};

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

pub(super) struct Writer {
    sender: mpsc::UnboundedSender<Job>,
}

impl Writer {
    pub fn spawn(mut connection: Connection, max_bytes: u64) -> Result<Self, Error> {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Job>();
        std::thread::Builder::new()
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
        Ok(Self { sender })
    }

    pub fn submit<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    ) -> oneshot::Receiver<Result<T, Error>> {
        let (reply, receiver) = oneshot::channel();
        let job: Job = Box::new(move |connection| {
            let _ = reply.send(work(connection));
        });
        let _ = self.sender.send(job);
        receiver
    }
}
