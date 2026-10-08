mod filter;
mod hub;
#[cfg(test)]
mod tests;

use crate::kernel::events::CoreEvent;
use hub::{Factory, Hub, Prepared, Timing};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, Prefix};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Runtime, State};

pub const MAX_ROOTS: usize = 200;
const DEBOUNCE: Duration = Duration::from_millis(300);
const HEALTH_CHECK: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Signal {
    Changed(String),
    Failed { reason: String },
    Lost { path: String, reason: String },
}

pub(crate) type Sink = Arc<dyn Fn(Signal) + Send + Sync>;

#[derive(Debug, thiserror::Error, PartialEq)]
pub(crate) enum WatchError {
    #[error("This set has {count} repositories; automatic refresh watches up to {limit}.")]
    TooManyRoots { count: usize, limit: usize },
    #[error("Automatic refresh could not start: {0}")]
    Start(String),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Skipped {
    path: String,
    reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    watched: usize,
    skipped: Vec<Skipped>,
    best_effort: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FailedPayload {
    reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LostPayload {
    path: String,
    reason: String,
}

#[derive(Default)]
struct Registry {
    sets: HashMap<String, Vec<String>>,
    hub: Option<Hub>,
}

impl Registry {
    fn held(&self) -> HashSet<String> {
        self.sets.values().flatten().cloned().collect()
    }

    fn is_held(&self, path: &str) -> bool {
        self.sets
            .values()
            .any(|roots| roots.iter().any(|root| root == path))
    }

    fn release(&mut self, previous: &[String]) {
        for path in previous.iter().filter(|path| !self.is_held(path)) {
            if let Some(hub) = &self.hub {
                hub.remove(path);
            }
        }
        if self.sets.is_empty() {
            self.hub = None;
        }
    }
}

pub struct Service {
    state: Mutex<Registry>,
    generation: AtomicU64,
    factory: Factory,
    sink: Sink,
    debounce: Duration,
    health: Duration,
}

type Candidate = Option<Result<Prepared, String>>;

impl Service {
    pub(crate) fn new(sink: Sink) -> Self {
        Self::with_parts(sink, hub::recommended(), DEBOUNCE, HEALTH_CHECK)
    }

    fn with_parts(sink: Sink, factory: Factory, debounce: Duration, health: Duration) -> Self {
        Self {
            state: Mutex::default(),
            generation: AtomicU64::new(0),
            factory,
            sink,
            debounce,
            health,
        }
    }

    pub(crate) fn watch(&self, set: &str, paths: &[String]) -> Result<Report, WatchError> {
        self.watch_in(set, paths, self.generation.load(Ordering::SeqCst))
    }

    fn watch_in(&self, set: &str, paths: &[String], generation: u64) -> Result<Report, WatchError> {
        let mut seen = HashSet::new();
        let unique: Vec<&String> = paths
            .iter()
            .filter(|path| seen.insert(path.as_str()))
            .collect();
        if unique.len() > MAX_ROOTS {
            self.unwatch(set);
            return Err(WatchError::TooManyRoots {
                count: unique.len(),
                limit: MAX_ROOTS,
            });
        }
        let held = self.lock().held();
        let mut candidates: Vec<Candidate> = unique
            .iter()
            .map(|path| (!held.contains(path.as_str())).then(|| prepare_root(path)))
            .collect();
        let mut registry = self.lock();
        if self.generation.load(Ordering::SeqCst) != generation {
            return Ok(Report::default());
        }
        let previous = registry.sets.remove(set).unwrap_or_default();
        let mut report = Report::default();
        let mut kept: Vec<String> = Vec::new();
        for (path, candidate) in unique.into_iter().zip(candidates.drain(..)) {
            match self.attach(&mut registry, &previous, path, candidate) {
                Ok(()) => {}
                Err(Attach::Refused(reason)) => {
                    report.skipped.push(Skipped {
                        path: path.clone(),
                        reason,
                    });
                    continue;
                }
                Err(Attach::Broken(reason)) => {
                    registry.release(&previous);
                    return Err(WatchError::Start(reason));
                }
            }
            if is_best_effort_location(Path::new(path)) {
                report.best_effort.push(path.clone());
            }
            kept.push(path.clone());
        }
        report.watched = kept.len();
        if !kept.is_empty() {
            registry.sets.insert(set.to_string(), kept);
        }
        registry.release(&previous);
        Ok(report)
    }

    fn attach(
        &self,
        registry: &mut Registry,
        previous: &[String],
        path: &str,
        candidate: Candidate,
    ) -> Result<(), Attach> {
        if previous.iter().any(|known| known == path) || registry.is_held(path) {
            return Ok(());
        }
        let prepared = candidate
            .unwrap_or_else(|| prepare_root(path))
            .map_err(Attach::Refused)?;
        if registry.hub.is_none() {
            let timing = Timing {
                debounce: self.debounce,
                health: self.health,
            };
            let hub = Hub::new(&self.factory, timing, self.sink.clone()).map_err(Attach::Broken)?;
            registry.hub = Some(hub);
        }
        let hub = registry
            .hub
            .as_ref()
            .ok_or_else(|| Attach::Broken("watcher missing".into()))?;
        hub.commit(prepared).map_err(Attach::Refused)
    }

    pub(crate) fn unwatch(&self, set: &str) {
        let mut registry = self.lock();
        let previous = registry.sets.remove(set).unwrap_or_default();
        registry.release(&previous);
    }

    pub(crate) fn stop_all(&self) {
        let mut registry = self.lock();
        self.generation.fetch_add(1, Ordering::SeqCst);
        registry.sets.clear();
        let hub = registry.hub.take();
        drop(registry);
        drop(hub);
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.lock().sets.len()
    }

    #[cfg(test)]
    pub(crate) fn root_count(&self) -> usize {
        self.lock().hub.as_ref().map_or(0, Hub::len)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Registry> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn prepare_root(path: &str) -> Result<Prepared, String> {
    match refusal(path) {
        Some(reason) => Err(reason),
        None => hub::prepare(path),
    }
}

enum Attach {
    Refused(String),
    Broken(String),
}

fn home_directory() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(name).map(std::path::PathBuf::from)
}

fn same_folder(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn refusal(path: &str) -> Option<String> {
    refusal_in(path, home_directory().as_deref())
}

fn refusal_in(path: &str, home: Option<&Path>) -> Option<String> {
    let folder = Path::new(path);
    if !folder.is_dir() {
        return Some("the folder was not found".into());
    }
    if let Err(reason) = crate::git::valid_path(path, true) {
        return Some(reason);
    }
    if folder.parent().is_none() {
        return Some("a drive or filesystem root is too broad to watch".into());
    }
    if home.is_some_and(|home| same_folder(folder, home)) {
        return Some("the home folder is too broad to watch".into());
    }
    None
}

fn is_best_effort_location(path: &Path) -> bool {
    let mut parts = path.components();
    let network = matches!(
        parts.next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
    );
    network
        || path.components().any(|part| match part {
            Component::Normal(name) => name
                .to_str()
                .is_some_and(|name| name.to_ascii_lowercase().starts_with("onedrive")),
            _ => false,
        })
}

pub(crate) fn tauri_sink<R: Runtime>(app: AppHandle<R>) -> Sink {
    Arc::new(move |signal| {
        let sent = match signal {
            Signal::Changed(path) => crate::events::publish_payload(
                &app,
                CoreEvent::RepoChanged,
                &ChangedPayload { path },
            ),
            Signal::Failed { reason } => crate::events::publish_payload(
                &app,
                CoreEvent::WatchFailed,
                &FailedPayload { reason },
            ),
            Signal::Lost { path, reason } => crate::events::publish_payload(
                &app,
                CoreEvent::WatchLost,
                &LostPayload { path, reason },
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
) -> Result<Report, String> {
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
