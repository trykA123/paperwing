use super::*;

pub(super) const PAGE_BYTES: usize = 1024 * 1024;
pub(super) const PAGE_ROWS: usize = 500;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Hint {
    Unavailable,
    Opaque,
    TypeConflict,
    LeftOnly,
    RightOnly,
    SameId,
    ChangedId,
    Folder,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRow {
    pub(super) id: String,
    pub(super) path: String,
    pub(super) left: Option<SideInfo>,
    pub(super) right: Option<SideInfo>,
    pub(super) hint: Hint,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum RowUpdate {
    Pending(PendingRow),
    Final(FileRow),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Resolving,
    Listing,
    Enriching,
    Complete,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub(super) raw: Summary,
    pub(super) display: Summary,
    pub(super) pending: usize,
    pub(super) rows: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub(super) id: String,
    pub(super) generation: u64,
    pub(super) state: State,
    pub(super) sequence: u64,
    pub(super) rows: Vec<RowUpdate>,
    pub(super) more: bool,
    pub(super) totals: Option<Totals>,
    pub(super) history: Option<History>,
    pub(super) snapshot: Option<Snapshot>,
    pub(super) outcome: Option<String>,
    pub(super) problem: Option<Problem>,
}

pub(super) struct Retained {
    pub listed: Option<Arc<listing::Listed>>,
    pub final_ids: std::collections::HashSet<String>,
    pub state: State,
    pub updates: Vec<RowUpdate>,
    pub totals: Option<Totals>,
    pub snapshot: Option<Snapshot>,
    pub problem: Option<Problem>,
    pub events: progress_events::Events,
    #[cfg(feature = "benchmark")]
    pub listed_timer: Option<crate::benchmark::Span>,
}

impl Retained {
    pub fn new(sink: Option<progress_events::Sink>) -> Self {
        Self {
            listed: None,
            final_ids: Default::default(),
            state: State::Resolving,
            updates: Vec::new(),
            totals: None,
            snapshot: None,
            problem: None,
            events: progress_events::Events::new(sink),
            #[cfg(feature = "benchmark")]
            listed_timer: Some(crate::benchmark::Span::new("compare.listed", "other")),
        }
    }

    pub fn page(
        &self,
        id: &str,
        generation: u64,
        after: u64,
        limit: usize,
    ) -> Result<Progress, Problem> {
        if limit == 0 || limit > PAGE_ROWS || after > self.updates.len() as u64 {
            return Err(Problem::new("limitExceeded", "Invalid progress page"));
        }
        let mut rows = Vec::new();
        let mut bytes = 0;
        for row in self.updates.iter().skip(after as usize).take(limit) {
            let size = serde_json::to_vec(row)
                .map_err(|_| Problem::new("internal", "Progress serialization failed"))?
                .len();
            if !rows.is_empty() && bytes + size > PAGE_BYTES {
                break;
            }
            bytes += size;
            rows.push(row.clone());
        }
        let sequence = after + rows.len() as u64;
        Ok(Progress {
            id: id.into(),
            generation,
            state: self.state,
            sequence,
            rows,
            more: sequence < self.updates.len() as u64,
            totals: self.totals.clone(),
            history: self.snapshot.as_ref().map(|view| view.history.clone()),
            snapshot: self.snapshot.clone(),
            outcome: self.problem.as_ref().and_then(outcome).map(str::to_owned),
            problem: self.problem.clone(),
        })
    }

    pub fn emit(&mut self, id: &str, generation: u64) {
        self.events.queue(progress_events::Notice {
            id: id.into(),
            generation,
            sequence: self.updates.len() as u64,
            state: self.state,
        });
    }
}

pub(super) fn outcome(problem: &Problem) -> Option<&'static str> {
    match problem.kind.as_str() {
        "unavailable" | "githubUnavailable" | "githubNotFound" | "githubRateLimited" => {
            Some("unavailable")
        }
        "invalidRef" => Some("invalidRef"),
        "missingLeft" => Some("missingLeft"),
        "missingRight" => Some("missingRight"),
        "networkError" => Some("networkError"),
        _ => None,
    }
}

pub(super) fn refresh_result(problem: Problem) -> Result<RefreshResult, Problem> {
    match outcome(&problem) {
        Some("unavailable") => Ok(RefreshResult::Unavailable { problem }),
        Some("invalidRef") => Ok(RefreshResult::InvalidRef { problem }),
        Some("missingLeft") => Ok(RefreshResult::MissingLeft { problem }),
        Some("missingRight") => Ok(RefreshResult::MissingRight { problem }),
        Some("networkError") => Ok(RefreshResult::NetworkError { problem }),
        _ => Err(problem),
    }
}
