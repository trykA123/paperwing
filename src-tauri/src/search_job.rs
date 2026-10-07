use crate::search::{
    dedupe, plan, validate_target, Match, Plan, RepoStatus, RepoTarget, SearchRequest, State,
};
use crate::search_engine::{Budget, GitGrep, RepoSearch, SearchEngine};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const CHUNK: usize = 200;
pub const MATCHES_EVENT: &str = "search-matches";
pub const REPO_EVENT: &str = "search-repo";
pub const DONE_EVENT: &str = "search-done";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MatchesPayload {
    pub id: u64,
    pub repo: String,
    pub matches: Vec<Match>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoPayload {
    pub id: u64,
    pub repo: String,
    pub status: RepoStatus,
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

#[derive(Clone)]
pub enum Outbound {
    Matches(MatchesPayload),
    Repo(RepoPayload),
    Done(DonePayload),
}

pub type Emit = Arc<dyn Fn(Outbound) + Send + Sync>;

pub struct Job {
    pub id: u64,
    pub concurrency: usize,
    pub cancel: Arc<AtomicBool>,
    pub send: Emit,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub perl: bool,
}

struct Shared {
    id: u64,
    plan: Plan,
    cancel: Arc<AtomicBool>,
    budget: Arc<Budget>,
    send: Emit,
    skipped_for_cap: AtomicBool,
}

impl Shared {
    fn emit_repo(&self, repo: &str, status: RepoStatus) {
        let payload = RepoPayload {
            id: self.id,
            repo: repo.to_string(),
            status,
        };
        (self.send)(Outbound::Repo(payload));
    }

    fn emit_matches(&self, repo: &str, matches: Vec<Match>) {
        for chunk in matches.chunks(CHUNK) {
            let payload = MatchesPayload {
                id: self.id,
                repo: repo.to_string(),
                matches: chunk.to_vec(),
            };
            (self.send)(Outbound::Matches(payload));
        }
    }
}

async fn checked(shared: &Arc<Shared>, target: &RepoTarget) -> Result<(), RepoStatus> {
    let (inner, target) = (shared.clone(), target.clone());
    let validation =
        tauri::async_runtime::spawn_blocking(move || validate_target(&target, &inner.plan));
    match validation.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(RepoStatus::without_matches(State::Failed, Some(error))),
        Err(_) => {
            return Err(RepoStatus::without_matches(
                State::Failed,
                Some("Validation failed".into()),
            ))
        }
    }
    if shared.cancel.load(Ordering::Relaxed) {
        return Err(RepoStatus::without_matches(State::Cancelled, None));
    }
    if shared.budget.is_capped() {
        shared.skipped_for_cap.store(true, Ordering::SeqCst);
        let reason = Some("Overall result limit reached".into());
        return Err(RepoStatus::without_matches(State::Skipped, reason));
    }
    Ok(())
}

async fn search_one(shared: Arc<Shared>, gate: Arc<Semaphore>, target: RepoTarget) -> RepoStatus {
    let Ok(_permit) = gate.acquire().await else {
        return RepoStatus::without_matches(State::Failed, Some("Search is unavailable".into()));
    };
    let (status, matches) = match checked(&shared, &target).await {
        Err(status) => (status, Vec::new()),
        Ok(()) => {
            let found = GitGrep
                .search(RepoSearch {
                    target: &target,
                    plan: &shared.plan,
                    budget: shared.budget.clone(),
                    cancel: shared.cancel.clone(),
                })
                .await;
            (found.status, found.matches)
        }
    };
    shared.emit_matches(&target.path, matches);
    shared.emit_repo(&target.path, status.clone());
    status
}

async fn guarded(shared: Arc<Shared>, gate: Arc<Semaphore>, target: RepoTarget) -> RepoStatus {
    let path = target.path.clone();
    let mut inner = Aborting(tokio::spawn(search_one(shared.clone(), gate, target)));
    match (&mut inner.0).await {
        Ok(status) => status,
        Err(_) => {
            let status = RepoStatus::without_matches(
                State::Failed,
                Some("Search crashed for this repository".into()),
            );
            shared.emit_repo(&path, status.clone());
            status
        }
    }
}

struct Aborting(tokio::task::JoinHandle<RepoStatus>);

impl Drop for Aborting {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct DoneGuard {
    id: u64,
    send: Emit,
    sent: bool,
}

impl DoneGuard {
    fn finish(&mut self, summary: Summary) {
        self.sent = true;
        (self.send)(Outbound::Done(DonePayload {
            id: self.id,
            summary,
        }));
    }
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        if !self.sent {
            let summary = Summary {
                failed: 1,
                cancelled: true,
                ..Summary::default()
            };
            (self.send)(Outbound::Done(DonePayload {
                id: self.id,
                summary,
            }));
        }
    }
}

pub async fn run_job(request: SearchRequest, job: Job) -> Result<Summary, String> {
    let plan = plan(&request)?;
    let mut done = DoneGuard {
        id: job.id,
        send: job.send.clone(),
        sent: false,
    };
    let repos = tauri::async_runtime::spawn_blocking(move || dedupe(request.repos)).await;
    let repos = repos.map_err(|_| "Could not read repository paths".to_string())?;
    let budget = Arc::new(Budget::new(plan.overall));
    let shared = Arc::new(Shared {
        id: job.id,
        plan,
        cancel: job.cancel.clone(),
        budget: budget.clone(),
        send: job.send.clone(),
        skipped_for_cap: AtomicBool::new(false),
    });
    let gate = Arc::new(Semaphore::new(job.concurrency.max(1)));
    let mut tasks = JoinSet::new();
    for target in repos.iter().cloned() {
        tasks.spawn(guarded(shared.clone(), gate.clone(), target));
    }
    let mut summary = Summary {
        repos: repos.len(),
        ..Summary::default()
    };
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(status) => {
                summary.matches += status.matches;
                summary.failed += usize::from(status.state == State::Failed);
            }
            Err(_) => summary.failed += 1,
        }
    }
    summary.capped = budget.is_capped() || shared.skipped_for_cap.load(Ordering::SeqCst);
    summary.cancelled = job.cancel.load(Ordering::Relaxed);
    done.finish(summary.clone());
    Ok(summary)
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
