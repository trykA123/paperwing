mod authority;
mod commands;
mod copy;
mod discard;
pub(crate) use discard::discard_restore;
mod recovery;
mod save;
mod tickets;

use crate::settings::Settings;
pub(crate) use commands::*;
#[cfg(test)]
pub(crate) use copy::AsyncContent;
#[cfg(test)]
pub(crate) use tickets::Request;

#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub comparisons: &'a crate::compare::Service,
    pub environment: &'a Environment,
}
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use tickets::Ticket;

const CONTENT_LIMIT: usize = 2 * 1024 * 1024;
const COPY_LIMIT: usize = 32 * 1024 * 1024;
const CONTRACT_WARNING: &str = "Linux checks bytes and identities immediately before atomic replacement. Another process can still change or move files after that check. Recovery backups are retained.";

type SettingsReader = Arc<dyn Fn() -> Result<Settings, String> + Send + Sync>;
#[derive(Clone)]
pub(crate) struct Environment {
    app_data: PathBuf,
    settings: SettingsReader,
}
impl Environment {
    pub(crate) fn load(&self) -> Result<Settings, String> {
        (self.settings)()
    }

    pub(super) fn revalidate(&self, expected: &Settings) -> Result<(), String> {
        let current = self.load()?;
        if current.workspace != expected.workspace
            || !current
                .sources
                .iter()
                .map(|source| (&source.id, &source.name))
                .eq(expected
                    .sources
                    .iter()
                    .map(|source| (&source.id, &source.name)))
        {
            return Err("Registered settings changed; reopen the comparison".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Service {
    tickets: Mutex<HashMap<String, Ticket>>,
    copies: Mutex<HashMap<String, Arc<copy::Plan>>>,
}
impl Service {
    pub(crate) async fn release_tickets(&self) {
        if let Ok(mut tickets) = self.tickets.lock() {
            for ticket in tickets.values() {
                ticket
                    .cancel
                    .store(true, std::sync::atomic::Ordering::Release);
            }
            tickets.clear();
        }
        if let Ok(mut copies) = self.copies.lock() {
            for plan in copies.values() {
                plan.cancel
                    .store(true, std::sync::atomic::Ordering::Release);
            }
            copies.clear();
        }
    }
}
fn lock<T>(state: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    state
        .lock()
        .map_err(|_| "Linux file service state is unavailable; reload the window".into())
}
fn unique() -> Result<String, String> {
    crate::linux_guard::storage::unique_name("t-").map_err(|error| error.to_string())
}
fn filesystem() -> Result<tokio::sync::RwLockWriteGuard<'static, ()>, String> {
    let guard = crate::git::filesystem_gate()
        .try_write()
        .map_err(|_| "Git or another write is running; retry after it finishes")?;
    if crate::clone::busy() {
        return Err("Git operation is still running; retry after it finishes".into());
    }
    Ok(guard)
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    id: String,
    root: PathBuf,
    path: String,
    existed: bool,
    stage: String,
    created_at: u128,
    root_volume: u32,
    root_index: u64,
    warning: Option<String>,
}
impl From<crate::linux_journal::RecoveryRecord> for Record {
    fn from(record: crate::linux_journal::RecoveryRecord) -> Self {
        Self {
            id: record.id,
            root: record.root,
            path: record.path,
            existed: record.existed,
            stage: record.stage,
            created_at: record.created_at,
            root_volume: 0,
            root_index: 0,
            warning: record.warning.map(str::to_string),
        }
    }
}

#[cfg(test)]
impl Environment {
    pub(crate) fn fixture(
        app_data: PathBuf,
        settings: impl Fn() -> Result<Settings, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            app_data,
            settings: Arc::new(settings),
        }
    }
}
#[cfg(test)]
impl Service {
    pub(crate) fn ticket_count(&self) -> usize {
        self.tickets.lock().unwrap().len()
    }
    pub(crate) fn copy_count(&self) -> usize {
        self.copies.lock().unwrap().len()
    }
}
