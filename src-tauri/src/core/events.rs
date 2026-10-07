use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    DiscoverBatch(Value),
    DiscoverDone(Value),
    SearchMatches(Value),
    SearchRepo(Value),
    SearchDone(Value),
    CloneProgress(Value),
    CloneFinished,
    LaunchRequest,
    CredentialChanged(Value),
    GitActivity(Value),
    DiagnosticsProgress(Value),
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
