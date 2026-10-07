pub(crate) use runner::CAPTURE_LIMIT;
pub(crate) mod batch_repository;
pub(crate) use runner::BatchReader;
mod runner;
pub use runner::{Activity, Captured, ClearedActivity, OutputPolicy, Request, attach, buffered, configure_sources, execute, execute_cancellable, execute_cancellable_input, execute_streaming, filesystem_gate, StdoutSink};
pub type ActivityOutput = runner::ActivityOutput;
use runner::configured_sources;
#[cfg(test)]
pub(crate) use runner::TEST_RUNNER_LOCK;
#[cfg(test)]
pub(crate) use runner::binary::BinaryOverride;
#[cfg(test)]
use runner::{ExitObserver, NEXT_ID, Observer, drain, execute_inner};

pub(crate) mod remote_refs;
pub use remote_refs::RefsResult;
#[cfg(test)]
use remote_refs::{ls_remote, natural_cmp};

mod repository_tree;
pub use repository_tree::RepositoryTree;
pub type TreeRef = repository_tree::TreeRef;
pub type TreeRemote = repository_tree::TreeRemote;
pub type TreeStash = repository_tree::TreeStash;
pub type TreeSubmodule = repository_tree::TreeSubmodule;

pub(crate) mod repo_command;
mod redaction;
mod remote_list;
mod validation;
pub use redaction::{last_error, redact, safe};
#[cfg(test)]
pub(crate) use runner::CredentialFixture;
pub use validation::{valid_path, valid_ref, valid_root, valid_url};

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "linux"))]
mod process_tests;
#[cfg(test)]
mod slow_tree_tests;

#[tauri::command]
pub async fn repository_tree(path: String) -> Result<RepositoryTree, String> {
    repository_tree::repository_tree(path).await
}

#[tauri::command]
pub async fn get_refs_many(urls: Vec<String>) -> Vec<RefsResult> {
    crate::providers::refs(urls).await
}

#[tauri::command]
pub fn activity_snapshot() -> Vec<Activity> { runner::activity_snapshot() }

#[tauri::command]
pub fn clear_activity() -> ClearedActivity { runner::clear_activity() }

#[tauri::command]
pub fn cancel_activity(id: String) -> bool { runner::cancel_activity(id) }
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn runner_idle() -> bool { runner::resources_idle() }
