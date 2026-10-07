use crate::discover::{scan, Event, FoundRepo, Limits, Summary};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const BATCH_LEN: usize = 25;
const BATCH_AGE: Duration = Duration::from_millis(50);
const MAX_RUNNING: usize = 4;
const MAX_DEPTH: u32 = 8;
pub const BATCH_EVENT: &str = "discover-batch";
pub const DONE_EVENT: &str = "discover-done";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BatchPayload {
    id: u64,
    repos: Vec<FoundRepo>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DonePayload {
    id: u64,
    summary: Summary,
}

pub enum Outbound {
    Batch(Vec<FoundRepo>),
    Done(Summary),
}

pub struct Batcher {
    items: Vec<FoundRepo>,
    since: Option<Instant>,
}

impl Batcher {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            since: None,
        }
    }

    pub fn push(&mut self, item: FoundRepo, now: Instant) -> Option<Vec<FoundRepo>> {
        self.since.get_or_insert(now);
        self.items.push(item);
        self.poll(now)
    }

    pub fn poll(&mut self, now: Instant) -> Option<Vec<FoundRepo>> {
        let due = self.items.len() >= BATCH_LEN
            || self
                .since
                .is_some_and(|since| now.duration_since(since) >= BATCH_AGE);
        due.then(|| self.take()).flatten()
    }

    pub fn take(&mut self) -> Option<Vec<FoundRepo>> {
        self.since = None;
        (!self.items.is_empty()).then(|| std::mem::take(&mut self.items))
    }
}

pub fn run_job(root: &Path, limits: Limits, cancel: &AtomicBool, send: &dyn Fn(Outbound)) {
    let mut batcher = Batcher::new();
    let summary = scan(root, limits, cancel, &mut |event| {
        let ready = match event {
            Event::Repo(repo) => batcher.push(repo, Instant::now()),
            Event::Visited => batcher.poll(Instant::now()),
        };
        if let Some(repos) = ready {
            send(Outbound::Batch(repos));
        }
    });
    if let Some(repos) = batcher.take() {
        send(Outbound::Batch(repos));
    }
    send(Outbound::Done(summary));
}

#[derive(Default)]
pub struct Service {
    next: AtomicU64,
    active: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
}

impl Service {
    fn register(&self) -> Result<(u64, Arc<AtomicBool>), String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "Folder scan registry is unavailable")?;
        if active.len() >= MAX_RUNNING {
            return Err("Too many folder scans are running; cancel one first".into());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let flag = Arc::new(AtomicBool::new(false));
        active.insert(id, flag.clone());
        Ok((id, flag))
    }

    fn cancel_all(&self) -> usize {
        let active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active
            .values()
            .for_each(|flag| flag.store(true, Ordering::Relaxed));
        active.len()
    }

    fn cancel(&self, id: u64) -> bool {
        let active = self.active.lock().ok();
        active
            .and_then(|active| active.get(&id).cloned())
            .is_some_and(|flag| {
                flag.store(true, Ordering::Relaxed);
                true
            })
    }
}

struct Slot {
    active: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
    id: u64,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active.remove(&self.id);
    }
}

pub fn chosen_folder(path: &str) -> Result<std::path::PathBuf, String> {
    crate::git::valid_path(path, true)?;
    let canonical = crate::platform::canonical_path(Path::new(path))?;
    if !canonical.is_dir() {
        return Err("Only folders can be scanned".into());
    }
    Ok(canonical)
}

#[tauri::command]
pub async fn discover_start(
    app: AppHandle,
    service: State<'_, Service>,
    path: String,
    max_depth: Option<u32>,
) -> Result<u64, String> {
    let root = chosen_folder(&path).map_err(|reason| format!("Cannot scan {path}: {reason}"))?;
    let limits = Limits {
        depth: max_depth
            .unwrap_or(Limits::default().depth)
            .clamp(1, MAX_DEPTH),
        ..Limits::default()
    };
    let (id, cancel) = service.register()?;
    let slot = Slot {
        active: service.active.clone(),
        id,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _slot = slot;
        let send = |outbound: Outbound| {
            let sent = match outbound {
                Outbound::Batch(repos) => app.emit(BATCH_EVENT, BatchPayload { id, repos }),
                Outbound::Done(summary) => app.emit(DONE_EVENT, DonePayload { id, summary }),
            };
            if sent.is_err() {
                cancel.store(true, Ordering::Relaxed);
            }
        };
        run_job(&root, limits, &cancel, &send);
    });
    Ok(id)
}

#[tauri::command]
pub async fn discover_cancel(service: State<'_, Service>, id: u64) -> Result<bool, String> {
    Ok(service.cancel(id))
}

#[tauri::command]
pub async fn discover_cancel_all(service: State<'_, Service>) -> Result<usize, String> {
    Ok(service.cancel_all())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Fixture;
    use std::cell::RefCell;

    fn repo(name: &str) -> FoundRepo {
        FoundRepo {
            path: name.into(),
            name: name.into(),
            kind: crate::discover::RepoKind::Normal,
            branch: None,
            detached: false,
            parent: None,
        }
    }

    #[test]
    fn batches_flush_at_twenty_five_repositories() {
        let mut batcher = Batcher::new();
        let now = Instant::now();
        assert!((0..24).all(|index| batcher.push(repo(&index.to_string()), now).is_none()));
        assert_eq!(
            batcher.push(repo("last"), now).map(|batch| batch.len()),
            Some(25)
        );
        assert!(batcher.take().is_none());
    }

    #[test]
    fn batches_flush_after_fifty_milliseconds() {
        let mut batcher = Batcher::new();
        let start = Instant::now();
        assert!(batcher.push(repo("a"), start).is_none());
        assert!(batcher.poll(start + Duration::from_millis(49)).is_none());
        assert_eq!(
            batcher
                .poll(start + Duration::from_millis(50))
                .map(|batch| batch.len()),
            Some(1)
        );
        assert!(batcher.poll(start + Duration::from_millis(500)).is_none());
    }

    #[test]
    fn a_job_streams_batches_then_one_summary() {
        let fixture = Fixture::new("job-stream");
        for index in 0..30 {
            let git = fixture.0.join(format!("r{index}")).join(".git");
            std::fs::create_dir_all(&git).unwrap();
        }
        let sent = RefCell::new(Vec::new());
        let cancel = AtomicBool::new(false);
        run_job(&fixture.0, Limits::default(), &cancel, &|outbound| {
            sent.borrow_mut().push(outbound)
        });
        let sent = sent.into_inner();
        let batched: usize = sent
            .iter()
            .map(|outbound| {
                if let Outbound::Batch(repos) = outbound {
                    repos.len()
                } else {
                    0
                }
            })
            .sum();
        assert_eq!(batched, 30);
        assert!(matches!(sent.last(), Some(Outbound::Done(summary)) if summary.repositories == 30));
        assert_eq!(
            sent.iter()
                .filter(|outbound| matches!(outbound, Outbound::Done(_)))
                .count(),
            1
        );
    }

    #[test]
    fn service_cancels_known_scans_and_limits_concurrency() {
        let service = Service::default();
        let started: Vec<_> = (0..MAX_RUNNING)
            .map(|_| service.register().unwrap())
            .collect();
        assert!(service.register().is_err());
        assert!(service.cancel(started[0].0));
        assert!(started[0].1.load(Ordering::Relaxed));
        assert!(!service.cancel(999));
        assert_eq!(service.cancel_all(), MAX_RUNNING);
        assert!(started.iter().all(|(_, flag)| flag.load(Ordering::Relaxed)));
    }

    #[test]
    fn a_slot_frees_its_registration_even_when_the_job_panics() {
        let service = Service::default();
        let (id, _) = service.register().unwrap();
        let slot = Slot {
            active: service.active.clone(),
            id,
        };
        let outcome = std::thread::spawn(move || {
            let _slot = slot;
            panic!("scan failed");
        })
        .join();
        assert!(outcome.is_err());
        assert_eq!(service.cancel_all(), 0);
    }

    #[test]
    fn chosen_folders_must_be_existing_directories() {
        let fixture = Fixture::new("job-chosen");
        assert!(chosen_folder(fixture.0.to_str().unwrap()).is_ok());
        assert!(chosen_folder(fixture.0.join("missing").to_str().unwrap()).is_err());
        assert!(chosen_folder(fixture.0.join(".skein-test-root").to_str().unwrap()).is_err());
        assert!(chosen_folder("relative/path").is_err());
    }
}
