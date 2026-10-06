mod commands;
mod git;
mod ops;
mod parse;
mod switch;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_restore;
#[cfg(test)]
mod tests_switch;

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::OwnedMutexGuard;

pub use commands::*;
pub use parse::StashEntry;
pub use switch::SwitchOutcome;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushOutcome {
    pub stashed: Option<String>,
    pub nothing_to_stash: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub applied: bool,
    pub stash_kept: bool,
    pub index_restored: bool,
    pub conflicted: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StashDiff {
    pub patch: String,
    pub truncated: bool,
    pub has_untracked: bool,
    pub notice: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Restore {
    Apply,
    Pop,
}

pub(crate) fn valid_oid(oid: &str) -> Result<(), String> {
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Invalid stash id".into())
    }
}

pub(crate) async fn lock_repository(path: &str) -> OwnedMutexGuard<()> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let entry = {
        let mut locks = LOCKS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks.entry(key).or_default().clone()
    };
    entry.lock_owned().await
}
