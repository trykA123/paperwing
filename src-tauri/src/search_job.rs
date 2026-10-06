use crate::search::{
    perl_supported, plan, search_repo, validate_target, Match, Mode, Plan, RepoStatus, RepoTarget,
    SearchRequest, State,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State as TauriState};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const CONCURRENCY: usize = 4;
const MAX_RUNNING: usize = 4;
pub const REPO_EVENT: &str = "search-repo";
pub const DONE_EVENT: &str = "search-done";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoPayload {
    pub id: u64,
    pub repo: String,
    pub status: RepoStatus,
    pub matches: Vec<Match>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub repos: usize,
    pub matches: usize,
    pub failed: usize,
    pub capped: bool,
    pub cancelled: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    pub id: u64,
    pub summary: Summary,
}

pub enum Outbound {
    Repo(RepoPayload),
    Done(DonePayload),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    perl: bool,
}

struct Shared {
    id: u64,
    plan: Plan,
    cancel: Arc<AtomicBool>,
    found: AtomicUsize,
    send: Arc<dyn Fn(Outbound) + Send + Sync>,
}

impl Shared {
    fn allowance(&self) -> usize {
        self.plan
            .overall
            .saturating_sub(self.found.load(Ordering::SeqCst))
    }

    fn emit(&self, repo: String, status: RepoStatus, matches: Vec<Match>) {
        (self.send)(Outbound::Repo(RepoPayload {
            id: self.id,
            repo,
            status,
            matches,
        }));
    }
}

async fn search_target(shared: &Shared, target: &RepoTarget) -> (RepoStatus, Vec<Match>) {
    if let Err(error) = validate_target(target, &shared.plan) {
        return (
            RepoStatus::without_matches(State::Failed, Some(error)),
            Vec::new(),
        );
    }
    if shared.cancel.load(Ordering::Relaxed) {
        return (
            RepoStatus::without_matches(State::Cancelled, None),
            Vec::new(),
        );
    }
    let allowance = shared.allowance();
    if allowance == 0 {
        let reason = Some("Overall result limit reached".into());
        return (
            RepoStatus::without_matches(State::Skipped, reason),
            Vec::new(),
        );
    }
    let found = search_repo(target, &shared.plan, allowance, shared.cancel.clone()).await;
    let (mut status, mut matches) = (found.status, found.matches);
    let before = shared.found.fetch_add(matches.len(), Ordering::SeqCst);
    let room = shared.plan.overall.saturating_sub(before);
    if matches.len() > room {
        matches.truncate(room);
        status.matches = room;
        status.truncated = true;
    }
    (status, matches)
}

async fn run_target(shared: Arc<Shared>, gate: Arc<Semaphore>, target: RepoTarget) -> RepoStatus {
    let Ok(_permit) = gate.acquire().await else {
        return RepoStatus::without_matches(State::Failed, Some("Search is unavailable".into()));
    };
    let (status, matches) = search_target(&shared, &target).await;
    shared.emit(target.path, status.clone(), matches);
    status
}

pub async fn run_job(
    id: u64,
    request: SearchRequest,
    concurrency: usize,
    cancel: Arc<AtomicBool>,
    send: Arc<dyn Fn(Outbound) + Send + Sync>,
) -> Result<Summary, String> {
    let plan = plan(&request)?;
    let shared = Arc::new(Shared {
        id,
        plan,
        cancel: cancel.clone(),
        found: AtomicUsize::new(0),
        send,
    });
    let gate = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut tasks = JoinSet::new();
    for target in request.repos.iter().cloned() {
        tasks.spawn(run_target(shared.clone(), gate.clone(), target));
    }
    let mut summary = Summary {
        repos: request.repos.len(),
        ..Summary::default()
    };
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(status) if status.state == State::Failed => summary.failed += 1,
            Ok(_) => {}
            Err(_) => summary.failed += 1,
        }
    }
    summary.matches = shared.found.load(Ordering::SeqCst).min(shared.plan.overall);
    summary.capped = shared.found.load(Ordering::SeqCst) >= shared.plan.overall;
    summary.cancelled = cancel.load(Ordering::Relaxed);
    (shared.send)(Outbound::Done(DonePayload {
        id,
        summary: summary.clone(),
    }));
    Ok(summary)
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
            .map_err(|_| "Search registry is unavailable")?;
        if active.len() >= MAX_RUNNING {
            return Err("Too many searches are running; cancel one first".into());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let flag = Arc::new(AtomicBool::new(false));
        active.insert(id, flag.clone());
        Ok((id, flag))
    }

    fn cancel(&self, id: u64) -> bool {
        let flag = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.get(&id).cloned());
        flag.is_some_and(|flag| {
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

#[tauri::command]
pub async fn search_capabilities() -> Capabilities {
    Capabilities {
        perl: perl_supported().await,
    }
}

#[tauri::command]
pub async fn search_start(
    app: AppHandle,
    service: TauriState<'_, Service>,
    request: SearchRequest,
) -> Result<u64, String> {
    plan(&request)?;
    if request.mode == Mode::Perl && !perl_supported().await {
        return Err("This Git build has no Perl-compatible regex support".into());
    }
    let (id, cancel) = service.register()?;
    let slot = Slot {
        active: service.active.clone(),
        id,
    };
    let stop = cancel.clone();
    let send: Arc<dyn Fn(Outbound) + Send + Sync> = Arc::new(move |outbound| {
        let sent = match outbound {
            Outbound::Repo(payload) => app.emit(REPO_EVENT, payload),
            Outbound::Done(payload) => app.emit(DONE_EVENT, payload),
        };
        if sent.is_err() {
            stop.store(true, Ordering::Relaxed);
        }
    });
    tauri::async_runtime::spawn(async move {
        let _slot = slot;
        if let Err(error) = run_job(id, request, CONCURRENCY, cancel, send.clone()).await {
            send(Outbound::Done(DonePayload {
                id,
                summary: Summary {
                    failed: 1,
                    ..Summary::default()
                },
            }));
            eprintln!("search {id} failed: {error}");
        }
    });
    Ok(id)
}

#[tauri::command]
pub async fn search_cancel(service: TauriState<'_, Service>, id: u64) -> Result<bool, String> {
    Ok(service.cancel(id))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
