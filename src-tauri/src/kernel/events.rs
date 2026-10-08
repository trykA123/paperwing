use serde::{Deserialize, Serialize};
use serde_json::value::{RawValue, to_raw_value};
use tokio::sync::broadcast;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum CoreEvent {
    RepoOpened { path: String },
    RepoCloned { path: String },
    BranchChanged { path: String, branch: String },
    PullRequestUpdated { source: String, repository: String },
    CiStarted { source: String, run: String },
    CiCompleted { source: String, run: String },
    IssueUpdated { source: String, issue: String },
    ProviderHealthChanged { source: String, healthy: bool },
    DiscoverBatch(EventPayload),
    DiscoverDone(EventPayload),
    SearchMatches(EventPayload),
    SearchRepo(EventPayload),
    SearchDone(EventPayload),
    CloneProgress(EventPayload),
    CloneFinished,
    LaunchRequest,
    CredentialChanged(EventPayload),
    GitActivity(EventPayload),
    DiagnosticsProgress(EventPayload),
    RepoChanged(EventPayload),
    WatchFailed(EventPayload),
    WatchLost(EventPayload),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventPayload(Box<RawValue>);

impl EventPayload {
    pub fn new(payload: &impl Serialize) -> Result<Self, serde_json::Error> {
        to_raw_value(payload).map(Self)
    }
}

impl PartialEq for EventPayload {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

#[derive(Clone)]
pub struct EventBus(broadcast::Sender<CoreEvent>);

impl Default for EventBus {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self(broadcast::channel(capacity).0)
    }

    /// Subscribers may lag; frontend delivery is synchronous and bypasses this bus.
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.0.subscribe()
    }

    pub fn publish(
        &self,
        event: CoreEvent,
    ) -> Result<usize, broadcast::error::SendError<CoreEvent>> {
        self.0.send(event)
    }
}
