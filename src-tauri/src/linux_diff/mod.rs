mod admission;
mod lease;
mod record;
use crate::linux_guard::{
    root::RootValue,
    storage::{self, PrivateDir},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone, Debug)]
pub(crate) struct Error {
    pub message: &'static str,
    pub cancelled: bool,
    retained: bool,
}
impl Error {
    fn unavailable(message: &'static str) -> Self {
        Self {
            message,
            cancelled: false,
            retained: false,
        }
    }
    fn retain(mut self) -> Self {
        self.retained = true;
        self
    }
    fn check(cancel: &AtomicBool) -> Result<(), Self> {
        if cancel.load(Ordering::Relaxed) {
            Err(Self {
                message: "Comparison cancelled",
                cancelled: true,
                retained: false,
            })
        } else {
            Ok(())
        }
    }
}
impl From<crate::linux_guard::Error> for Error {
    fn from(error: crate::linux_guard::Error) -> Self {
        Self {
            message: error.message,
            cancelled: false,
            retained: error.kind == crate::linux_guard::ErrorKind::Accounting,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for Error {}
#[cfg(test)]
type StorageHook = Arc<dyn Fn(&str) -> Result<(), crate::linux_guard::Error> + Send + Sync>;
pub(crate) struct Storage {
    app_data: PathBuf,
    namespace: std::sync::OnceLock<PrivateDir>,
    work: Arc<Semaphore>,
    policy: admission::Policy,
    #[cfg(test)]
    hook: Option<StorageHook>,
}
impl Storage {
    pub(crate) fn new(app_data: PathBuf) -> Result<Self, Error> {
        storage::diff_path(&app_data)?;
        storage::diff_path(&app_data.join("linux-diff-v1"))?;
        Ok(Self {
            app_data,
            namespace: std::sync::OnceLock::new(),
            work: Arc::new(Semaphore::new(4)),
            policy: admission::Policy::default(),
            #[cfg(test)]
            hook: None,
        })
    }
    async fn permit(&self, cancel: &AtomicBool) -> Result<OwnedSemaphorePermit, Error> {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error {
                    message: "Comparison cancelled",
                    cancelled: true,
                    retained: false,
                });
            }
            tokio::select! {
                result=self.work.clone().acquire_owned() => return result.map_err(|_| Error {message:"Private diff work is unavailable",cancelled:false,retained:false}),
                _=tokio::time::sleep(std::time::Duration::from_millis(25))=>{}
            }
        }
    }
    pub(crate) async fn capture(
        &self,
        roots: Vec<crate::paths::ReadRoot>,
        cancel: &AtomicBool,
    ) -> Result<Vec<RootValue>, Error> {
        if roots.is_empty() || roots.len() > 2 {
            return Err(Error {
                message: "Private diff source authority is missing",
                cancelled: false,
                retained: false,
            });
        }
        let permit = self.permit(cancel).await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            roots
                .iter()
                .map(|root| {
                    root.linux_value().map_err(|_| Error {
                        message: "Private diff source authority changed",
                        cancelled: false,
                        retained: false,
                    })
                })
                .collect()
        })
        .await
        .map_err(|_| Error {
            message: "Private diff authority work failed",
            cancelled: false,
            retained: false,
        })?
    }
    fn initialize(&self, roots: &[RootValue]) -> Result<PrivateDir, Error> {
        if self.policy.bytes < storage::DIFF_NAMESPACE_BYTES + 4096 {
            return Err(Error::unavailable(
                "Private diff storage quota is exhausted",
            ));
        }
        if let Some(saved) = self.namespace.get() {
            saved.revalidate()?;
        }
        let current = PrivateDir::initialize_diff(&self.app_data, roots)?;
        let saved = self.namespace.get_or_init(|| current.clone());
        if current.identity()? != saved.identity()? {
            return Err(Error::unavailable(
                "Private diff namespace identity changed",
            ));
        }
        Ok(saved.clone())
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
struct HookGuard(Option<storage::DiffStorageHook>);
#[cfg(test)]
impl Storage {
    fn install_hook(&self) -> HookGuard {
        let hook = self.hook.clone();
        HookGuard(storage::DIFF_STORAGE_HOOK.with(|slot| {
            slot.replace(
                hook.map(|hook| {
                    Box::new(move |phase: &str| hook(phase)) as storage::DiffStorageHook
                }),
            )
        }))
    }
}
#[cfg(test)]
impl Drop for HookGuard {
    fn drop(&mut self) {
        storage::DIFF_STORAGE_HOOK.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}
