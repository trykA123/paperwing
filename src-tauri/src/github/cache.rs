use super::{RepoList, Source};
use crate::store::{
    listings::{self, Listing},
    Error, Store,
};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

static EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scope {
    source_id: String,
    configuration: String,
}

#[derive(Clone)]
pub(super) struct ListingRequest {
    source_id: String,
    scope: Scope,
    revision: u64,
    epoch: u64,
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

fn epochs() -> &'static Mutex<HashMap<String, u64>> {
    EPOCHS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn scope_configuration(source: &Source) -> Result<String, String> {
    serde_json::to_string(&(
        &source.kind,
        &source.host,
        &source.orgs,
        source.credential_managed,
    ))
    .map_err(|error| error.to_string())
}

impl ListingRequest {
    pub fn new(source: &Source, refresh: bool) -> Result<Self, String> {
        let revision = crate::credentials::metadata_revision(source)?;
        let mut epochs = epochs()
            .lock()
            .map_err(|_| "Repository cache state is unavailable")?;
        let epoch = epochs.entry(source.id.clone()).or_default();
        if refresh {
            *epoch = epoch.wrapping_add(1);
        }
        let configuration = scope_configuration(source)?;
        Ok(Self {
            source_id: source.id.clone(),
            scope: Scope {
                source_id: source.id.clone(),
                configuration,
            },
            revision,
            epoch: *epoch,
        })
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn if_current<T>(&self, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        crate::credentials::if_current(&self.source_id, self.revision, || {
            let epochs = epochs()
                .lock()
                .map_err(|_| "Repository cache state is unavailable")?;
            if epochs.get(&self.source_id).copied().unwrap_or(0) != self.epoch {
                return Err("Repository metadata changed; retry the request".into());
            }
            action()
        })
        .ok_or_else(|| "Source credentials changed; reload repositories".to_string())?
    }

    pub async fn read_stale(&self, store: Store) -> Result<Option<RepoList>, String> {
        let request = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let stored = store
                .read_blocking(|connection| listings::get(connection, &request.source_id))
                .ok()
                .flatten();
            request.if_current(|| {
                Ok(stored.and_then(|listing| read_cached(listing, &request.scope, now())))
            })
        })
        .await
        .map_err(|_| "Repository cache read task failed".to_string())?
    }

    pub async fn finish(
        &self,
        store: Store,
        login: Option<String>,
        list: &RepoList,
    ) -> Result<(), String> {
        let request = self.clone();
        let list = list.clone();
        let queued = tauri::async_runtime::spawn_blocking(move || {
            request.if_current(|| Ok(request.enqueue(&store, login, list)))
        }).await.map_err(|_| "Repository cache write task failed".to_string())??;
        let outcome = match queued {
            Ok(receiver) => receiver
                .await
                .map_err(|_| Error::Unavailable)
                .and_then(|done| done),
            Err(error) => Err(error),
        };
        if let Err(reason) = outcome {
            eprintln!("Repository cache not updated: {reason}");
        }
        Ok(())
    }

    fn enqueue(
        &self,
        store: &Store,
        login: Option<String>,
        list: RepoList,
    ) -> Result<oneshot::Receiver<Result<(), Error>>, Error> {
        let source_id = self.source_id.clone();
        if !list.errors.is_empty() {
            return store.enqueue(move |connection| {
                listings::remove_other_login(connection, &source_id, login.as_deref())
            });
        }
        let partial = list.partial;
        let configuration = self.scope.configuration.clone();
        let listing = Listing {
            source_id,
            scope: if partial {
                partial_scope(&configuration)
            } else {
                configuration.clone()
            },
            login,
            fetched_at: list.fetched_at,
            version: listings::VERSION,
            repos: list.repos,
        };
        store.enqueue(move |connection| {
            if partial && has_fuller_listing(connection, &listing, &configuration) {
                return Ok(());
            }
            listings::put(connection, &listing)
        })
    }
}

const PARTIAL_MARK: &str = "\u{0}partial";

fn partial_scope(configuration: &str) -> String {
    format!("{configuration}{PARTIAL_MARK}")
}

fn has_fuller_listing(
    connection: &mut rusqlite::Connection,
    listing: &Listing,
    configuration: &str,
) -> bool {
    listings::get(connection, &listing.source_id)
        .ok()
        .flatten()
        .is_some_and(|stored| {
            stored.version == listings::VERSION
                && stored.scope == configuration
                && stored.login == listing.login
                && stored.repos.len() > listing.repos.len()
        })
}

fn read_cached(mut listing: Listing, scope: &Scope, now: u64) -> Option<RepoList> {
    let partial = listing.scope == partial_scope(&scope.configuration);
    if partial {
        listing.scope = scope.configuration.clone();
    }
    if listing.version != listings::VERSION
        || listing.source_id != scope.source_id
        || listing.scope != scope.configuration
        || listing.fetched_at > now
    {
        return None;
    }
    let (_, _, owners, _): (String, String, Vec<String>, bool) =
        serde_json::from_str(&scope.configuration).ok()?;
    if listing.repos.iter().any(|repo| {
        repo.source != scope.source_id
            || repo.id != format!("{}:{}/{}", scope.source_id, repo.org, repo.name)
            || !owners
                .iter()
                .any(|owner| owner.eq_ignore_ascii_case(&repo.org))
    }) {
        return None;
    }
    Some(RepoList {
        repos: listing.repos,
        fetched_at: listing.fetched_at,
        errors: Vec::new(),
        stale: true,
        warnings: if partial {
            vec!["Cached list is incomplete".into()]
        } else {
            Vec::new()
        },
        partial,
    })
}

#[cfg(test)]
mod tests;
