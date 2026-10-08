mod filter;
mod session;
#[cfg(test)]
mod tests;

use crate::kernel::events::CoreEvent;
use serde::Serialize;
use session::{Factory, Options, Root, Session};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Runtime, State};

pub const MAX_ROOTS: usize = 200;
const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Signal {
    Changed(String),
    Failed { set: String, reason: String },
}

pub(crate) type Sink = Arc<dyn Fn(Signal) + Send + Sync>;

#[derive(Debug, thiserror::Error, PartialEq)]
pub(crate) enum WatchError {
    #[error("This set has {count} repositories; automatic refresh watches up to {limit}.")]
    TooManyRoots { count: usize, limit: usize },
    #[error("Automatic refresh could not watch {0}")]
    Start(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FailedPayload {
    set_id: String,
    reason: String,
}

pub struct Service {
    sessions: Mutex<HashMap<String, Session>>,
    factory: Factory,
    sink: Sink,
    debounce: Duration,
}

impl Service {
    pub(crate) fn new(sink: Sink) -> Self {
        Self::with_parts(sink, session::recommended(), DEBOUNCE)
    }

    fn with_parts(sink: Sink, factory: Factory, debounce: Duration) -> Self {
        Self {
            sessions: Mutex::default(),
            factory,
            sink,
            debounce,
        }
    }

    pub(crate) fn watch(&self, set: &str, paths: &[String]) -> Result<usize, WatchError> {
        let unique: Vec<&String> = {
            let mut seen = HashSet::new();
            paths
                .iter()
                .filter(|path| seen.insert(path.as_str()))
                .collect()
        };
        if unique.len() > MAX_ROOTS {
            self.unwatch(set);
            return Err(WatchError::TooManyRoots {
                count: unique.len(),
                limit: MAX_ROOTS,
            });
        }
        self.unwatch(set);
        if unique.is_empty() {
            return Ok(0);
        }
        let roots = unique
            .iter()
            .map(|path| Root {
                path: PathBuf::from(path.as_str()),
                label: (*path).clone(),
            })
            .collect::<Vec<_>>();
        let count = roots.len();
        let options = Options {
            set: set.to_string(),
            debounce: self.debounce,
            sink: self.sink.clone(),
        };
        let session = Session::start(roots, options, &self.factory).map_err(WatchError::Start)?;
        let replaced = self.lock().insert(set.to_string(), session);
        drop(replaced);
        Ok(count)
    }

    pub(crate) fn unwatch(&self, set: &str) {
        let removed = self.lock().remove(set);
        drop(removed);
    }

    pub(crate) fn stop_all(&self) {
        let removed: Vec<Session> = self.lock().drain().map(|(_, session)| session).collect();
        drop(removed);
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub(crate) fn tauri_sink<R: Runtime>(app: AppHandle<R>) -> Sink {
    Arc::new(move |signal| {
        let sent = match signal {
            Signal::Changed(path) => crate::events::publish_payload(
                &app,
                CoreEvent::RepoChanged,
                &ChangedPayload { path },
            ),
            Signal::Failed { set, reason } => crate::events::publish_payload(
                &app,
                CoreEvent::WatchFailed,
                &FailedPayload {
                    set_id: set,
                    reason,
                },
            ),
        };
        if let Err(error) = sent {
            eprintln!("Automatic refresh event was not delivered: {error}");
        }
    })
}

#[tauri::command]
pub async fn watch_set(
    service: State<'_, Arc<Service>>,
    set_id: String,
    roots: Vec<String>,
) -> Result<usize, String> {
    let roots: Vec<String> = roots
        .into_iter()
        .filter(|root| std::path::Path::new(root).is_dir())
        .collect();
    for root in &roots {
        crate::git::valid_path(root, true)?;
    }
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.watch(&set_id, &roots))
        .await
        .map_err(|_| "Automatic refresh stopped unexpectedly".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn unwatch_set(service: State<'_, Arc<Service>>, set_id: String) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.unwatch(&set_id))
        .await
        .map_err(|_| "Automatic refresh stopped unexpectedly".to_string())
}
