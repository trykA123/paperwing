use super::{RepoList, Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 2;
static SESSION: OnceLock<String> = OnceLock::new();
static EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Scope {
    source_id: String,
    session: String,
    configuration: String,
    revision: u64,
    epoch: u64,
    metadata_epoch: u64,
    pub login: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CachedListing {
    version: u32,
    scope: Scope,
    list: RepoList,
}

#[derive(Clone)]
pub(super) struct ListingRequest {
    source_id: String,
    scope: Scope,
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

fn epochs() -> &'static Mutex<HashMap<String, u64>> {
    EPOCHS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl ListingRequest {
    pub fn new(source: &Source, refresh: bool, metadata_epoch: u64) -> Result<Self, String> {
        let revision = crate::credentials::metadata_revision(source)?;
        let mut epochs = epochs()
            .lock()
            .map_err(|_| "Repository cache state is unavailable")?;
        let epoch = epochs.entry(source.id.clone()).or_default();
        if refresh {
            *epoch = epoch.wrapping_add(1);
        }
        let session = SESSION
            .get_or_init(|| {
                format!(
                    "{}-{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |time| time.as_nanos())
                )
            })
            .clone();
        let configuration = serde_json::to_string(&(
            &source.kind,
            &source.host,
            &source.orgs,
            source.credential_managed,
        ))
        .map_err(|error| error.to_string())?;
        Ok(Self {
            source_id: source.id.clone(),
            scope: Scope {
                source_id: source.id.clone(),
                session,
                configuration,
                revision,
                epoch: *epoch,
                metadata_epoch,
                login: None,
            },
        })
    }

    pub fn revision(&self) -> u64 {
        self.scope.revision
    }

    pub fn scope(&self, login: Option<String>) -> Scope {
        Scope {
            login,
            ..self.scope.clone()
        }
    }

    fn if_current<T>(&self, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        crate::credentials::if_current(&self.source_id, self.scope.revision, || {
            let epochs = epochs()
                .lock()
                .map_err(|_| "Repository cache state is unavailable")?;
            if epochs.get(&self.source_id).copied().unwrap_or(0) != self.scope.epoch {
                return Err("Repository metadata changed; retry the request".into());
            }
            action()
        })
        .ok_or_else(|| "Source credentials changed; reload repositories".to_string())?
    }

    pub async fn read_cache(
        &self,
        file: PathBuf,
        scope: Scope,
    ) -> Result<Option<RepoList>, String> {
        let request = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            request.if_current(|| {
                let text = match std::fs::metadata(&file) {
                    Ok(metadata) if metadata.len() <= 16 * 1024 * 1024 => {
                        std::fs::read_to_string(file).ok()
                    }
                    _ => None,
                };
                Ok(text.and_then(|text| read_cached(&text, &scope, now())))
            })
        })
        .await
        .map_err(|_| "Repository cache read task failed".to_string())?
    }

    pub async fn finish(&self, file: PathBuf, scope: Scope, list: &RepoList) -> Result<(), String> {
        let request = self.clone();
        let list = list.clone();
        tauri::async_runtime::spawn_blocking(move || {
            request.if_current(|| {
                if !list.errors.is_empty() {
                    return Ok(());
                }
                let directory = file
                    .parent()
                    .ok_or("Repository cache directory is unavailable")?;
                std::fs::create_dir_all(directory)
                    .map_err(|_| "Cannot create repository cache directory")?;
                let text = serde_json::to_string(&CachedListing {
                    version: VERSION,
                    scope,
                    list,
                })
                .map_err(|error| error.to_string())?;
                let temporary = file.with_extension("json.tmp");
                std::fs::write(&temporary, text).map_err(|_| "Cannot write repository cache")?;
                std::fs::rename(&temporary, file)
                    .map_err(|_| "Cannot publish repository cache".to_string())
            })
        })
        .await
        .map_err(|_| "Repository cache write task failed".to_string())?
    }
}

fn read_cached(text: &str, scope: &Scope, now: u64) -> Option<RepoList> {
    let cached: CachedListing = serde_json::from_str(text).ok()?;
    if cached.version != VERSION
        || cached.scope != *scope
        || cached.list.fetched_at > now
        || !cached.list.errors.is_empty()
    {
        return None;
    }
    let (_, _, owners, _): (String, String, Vec<String>, bool) =
        serde_json::from_str(&scope.configuration).ok()?;
    if cached.list.repos.iter().any(|repo| {
        repo.source != scope.source_id
            || repo.id != format!("{}:{}/{}", scope.source_id, repo.org, repo.name)
            || !owners
                .iter()
                .any(|owner| owner.eq_ignore_ascii_case(&repo.org))
    }) {
        return None;
    }
    Some(cached.list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        serde_json::from_value(serde_json::json!({"id":"cache-fixture","name":"admin","kind":"github","host":"github.com","orgs":["admin"]})).unwrap()
    }

    #[test]
    fn legacy_corrupt_future_and_other_scopes_are_misses() {
        let scope = ListingRequest::new(&source(), false, 0)
            .unwrap()
            .scope(Some("admin".into()));
        let cached = CachedListing {
            version: VERSION,
            scope: scope.clone(),
            list: RepoList {
                fetched_at: 10,
                ..Default::default()
            },
        };
        let text = serde_json::to_string(&cached).unwrap();
        assert!(read_cached(&text, &scope, 10).is_some());
        assert!(read_cached(&text, &scope, 9).is_none());
        assert!(read_cached("{broken", &scope, 10).is_none());
        assert!(read_cached("{\"repos\":[],\"fetchedAt\":10,\"errors\":[]}", &scope, 10).is_none());
        for changed in [
            Scope {
                source_id: "another-source".into(),
                ..scope.clone()
            },
            Scope {
                session: "another-process".into(),
                ..scope.clone()
            },
            Scope {
                configuration: "edited host or owner".into(),
                ..scope.clone()
            },
            Scope {
                revision: 1,
                ..scope.clone()
            },
            Scope {
                epoch: scope.epoch + 1,
                ..scope.clone()
            },
            Scope {
                metadata_epoch: 1,
                ..scope.clone()
            },
            Scope {
                login: None,
                ..scope.clone()
            },
        ] {
            assert!(read_cached(&text, &changed, 10).is_none());
        }
        let old = serde_json::to_string(&CachedListing {
            version: 1,
            ..cached
        })
        .unwrap();
        assert!(read_cached(&old, &scope, 10).is_none());
        assert!(!text.contains("token"));
    }

    #[test]
    fn disk_rows_cannot_leak_repositories_from_another_source_or_owner() {
        let scope = ListingRequest::new(&source(), false, 0)
            .unwrap()
            .scope(Some("admin".into()));
        let repo: super::super::Repo = serde_json::from_value(serde_json::json!({
            "id":"cache-fixture:admin/repo", "source":"cache-fixture", "org":"admin", "name":"repo",
            "description":"", "url":"git@github.com:admin/repo.git", "defaultBranch":"main", "pushedAt":"", "archived":false
        })).unwrap();
        for invalid in [
            super::super::Repo {
                source: "another-source".into(),
                ..repo.clone()
            },
            super::super::Repo {
                org: "another-owner".into(),
                ..repo.clone()
            },
            super::super::Repo {
                id: "incompatible-id".into(),
                ..repo.clone()
            },
        ] {
            let cached = CachedListing {
                version: VERSION,
                scope: scope.clone(),
                list: RepoList {
                    repos: vec![invalid],
                    fetched_at: 10,
                    errors: vec![],
                },
            };
            assert!(read_cached(&serde_json::to_string(&cached).unwrap(), &scope, 10).is_none());
        }
    }

    #[tokio::test]
    async fn forced_refresh_prevents_an_old_request_from_refilling_disk() {
        let mut source = source();
        source.id = "cache-force-fixture".into();
        let fixture = crate::platform::Fixture::new("metadata-cache");
        let file = fixture.0.join("listing.json");
        let older = ListingRequest::new(&source, false, 0).unwrap();
        let newer = ListingRequest::new(&source, true, 0).unwrap();
        let list = RepoList {
            fetched_at: now(),
            ..Default::default()
        };
        newer
            .finish(file.clone(), newer.scope(Some("admin".into())), &list)
            .await
            .unwrap();
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(older
            .finish(file.clone(), older.scope(None), &list)
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
        let backup = fixture.0.join("listing-backup.json");
        std::fs::copy(&file, &backup).unwrap();
        std::fs::write(&file, "{corrupt").unwrap();
        assert!(newer
            .read_cache(file.clone(), newer.scope(Some("admin".into())))
            .await
            .unwrap()
            .is_none());
        std::fs::copy(&backup, &file).unwrap();
        assert!(newer
            .read_cache(file, newer.scope(Some("admin".into())))
            .await
            .unwrap()
            .is_some());
    }
}
