use super::{RepoList, Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 3;
static EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Scope {
    source_id: String,
    configuration: String,
}

#[derive(Serialize, Deserialize)]
struct CachedListing {
    version: u32,
    scope: Scope,
    login: Option<String>,
    list: RepoList,
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

    pub async fn read_stale(&self, file: PathBuf) -> Result<Option<RepoList>, String> {
        let request = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            request.if_current(|| {
                let text = match std::fs::metadata(&file) {
                    Ok(metadata) if metadata.len() <= 16 * 1024 * 1024 => {
                        std::fs::read_to_string(file).ok()
                    }
                    _ => None,
                };
                Ok(text.and_then(|text| read_cached(&text, &request.scope, now())))
            })
        })
        .await
        .map_err(|_| "Repository cache read task failed".to_string())?
    }

    pub async fn finish(
        &self,
        file: PathBuf,
        login: Option<String>,
        list: &RepoList,
    ) -> Result<(), String> {
        let request = self.clone();
        let list = list.clone();
        tauri::async_runtime::spawn_blocking(move || {
            request.if_current(|| {
                let scope = request.scope.clone();
                if list.errors.is_empty() {
                    write_listing(&file, scope, login, list)
                } else {
                    discard_other_login(&file, &login)
                }
                .unwrap_or_else(|reason| eprintln!("Repository cache not updated: {reason}"));
                Ok(())
            })
        })
        .await
        .map_err(|_| "Repository cache write task failed".to_string())?
    }
}

fn write_listing(
    file: &PathBuf,
    scope: Scope,
    login: Option<String>,
    list: RepoList,
) -> Result<(), String> {
    let directory = file
        .parent()
        .ok_or("Repository cache directory is unavailable")?;
    std::fs::create_dir_all(directory).map_err(|_| "Cannot create repository cache directory")?;
    let text = serde_json::to_string(&CachedListing {
        version: VERSION,
        scope,
        login,
        list,
    })
    .map_err(|error| error.to_string())?;
    let temporary = file.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|_| "Cannot write repository cache")?;
    std::fs::rename(&temporary, file).map_err(|_| "Cannot publish repository cache".to_string())
}

fn discard_other_login(file: &PathBuf, login: &Option<String>) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Ok(());
    };
    let stored = serde_json::from_str::<CachedListing>(&text).map(|cached| cached.login);
    if stored.is_ok_and(|stored| stored != *login) {
        std::fs::remove_file(file).map_err(|_| "Cannot remove stale repository cache")?;
    }
    Ok(())
}

fn read_cached(text: &str, scope: &Scope, now: u64) -> Option<RepoList> {
    let mut cached: CachedListing = serde_json::from_str(text).ok()?;
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
    cached.list.stale = true;
    Some(cached.list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        serde_json::from_value(serde_json::json!({"id":"cache-fixture","name":"admin","kind":"github","host":"github.com","orgs":["admin"]})).unwrap()
    }

    fn scope() -> Scope {
        ListingRequest::new(&source(), false).unwrap().scope
    }

    fn cached(scope: Scope, list: RepoList) -> String {
        serde_json::to_string(&CachedListing {
            version: VERSION,
            scope,
            login: Some("admin".into()),
            list,
        })
        .unwrap()
    }

    fn repo(source_id: &str, org: &str, name: &str) -> super::super::Repo {
        serde_json::from_value(serde_json::json!({
            "id":format!("{source_id}:{org}/{name}"), "source":source_id, "org":org, "name":name,
            "description":"", "url":format!("git@github.com:{org}/{name}.git"), "defaultBranch":"main", "pushedAt":"", "archived":false
        })).unwrap()
    }

    #[test]
    fn legacy_corrupt_future_and_other_scopes_are_misses() {
        let scope = scope();
        let list = RepoList {
            fetched_at: 10,
            ..Default::default()
        };
        let text = cached(scope.clone(), list);
        assert!(read_cached(&text, &scope, 10).is_some_and(|list| list.stale));
        assert!(read_cached(&text, &scope, 9).is_none());
        assert!(read_cached("{broken", &scope, 10).is_none());
        assert!(read_cached("{\"repos\":[],\"fetchedAt\":10,\"errors\":[]}", &scope, 10).is_none());
        for changed in [
            Scope {
                source_id: "another-source".into(),
                ..scope.clone()
            },
            Scope {
                configuration: "edited host or owner".into(),
                ..scope.clone()
            },
        ] {
            assert!(read_cached(&text, &changed, 10).is_none());
        }
        let old = text.replace("\"version\":3", "\"version\":2");
        assert!(read_cached(&old, &scope, 10).is_none());
        assert!(!text.contains("token"));
    }

    #[test]
    fn disk_rows_cannot_leak_repositories_from_another_source_or_owner() {
        let scope = scope();
        let valid = repo("cache-fixture", "admin", "repo");
        for invalid in [
            super::super::Repo {
                source: "another-source".into(),
                ..valid.clone()
            },
            super::super::Repo {
                org: "another-owner".into(),
                ..valid.clone()
            },
            super::super::Repo {
                id: "incompatible-id".into(),
                ..valid
            },
        ] {
            let list = RepoList {
                repos: vec![invalid],
                fetched_at: 10,
                ..Default::default()
            };
            assert!(read_cached(&cached(scope.clone(), list), &scope, 10).is_none());
        }
    }

    #[tokio::test]
    async fn disk_rows_survive_a_new_process_and_are_served_stale() {
        let mut source = source();
        source.id = "cache-stale-fixture".into();
        let file = crate::platform::Fixture::new("metadata-cache-stale")
            .0
            .join("listing.json");
        let list = RepoList {
            repos: vec![repo(&source.id, "admin", "repo")],
            fetched_at: now(),
            ..Default::default()
        };
        let request = ListingRequest::new(&source, false).unwrap();
        request
            .finish(file.clone(), Some("admin".into()), &list)
            .await
            .unwrap();
        let next_process = ListingRequest::new(&source, false).unwrap();
        let stale = next_process.read_stale(file).await.unwrap().unwrap();
        assert!(stale.stale);
        assert_eq!(stale.repos.len(), 1);
    }

    #[tokio::test]
    async fn cache_write_failures_do_not_fail_a_successful_listing() {
        let mut source = source();
        source.id = "cache-io-fixture".into();
        let fixture = crate::platform::Fixture::new("metadata-cache-io");
        let blocker = fixture.0.join("not-a-directory");
        std::fs::write(&blocker, "file").unwrap();
        let request = ListingRequest::new(&source, false).unwrap();
        let list = RepoList {
            fetched_at: now(),
            ..Default::default()
        };
        request
            .finish(blocker.join("listing.json"), None, &list)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn forced_refresh_prevents_an_old_request_from_refilling_disk() {
        let mut source = source();
        source.id = "cache-force-fixture".into();
        let fixture = crate::platform::Fixture::new("metadata-cache");
        let file = fixture.0.join("listing.json");
        let older = ListingRequest::new(&source, false).unwrap();
        let newer = ListingRequest::new(&source, true).unwrap();
        let list = RepoList {
            fetched_at: now(),
            ..Default::default()
        };
        newer
            .finish(file.clone(), Some("admin".into()), &list)
            .await
            .unwrap();
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(older.finish(file.clone(), None, &list).await.is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
        let backup = fixture.0.join("listing-backup.json");
        std::fs::copy(&file, &backup).unwrap();
        std::fs::write(&file, "{corrupt").unwrap();
        assert!(newer.read_stale(file.clone()).await.unwrap().is_none());
        std::fs::copy(&backup, &file).unwrap();
        assert!(newer.read_stale(file).await.unwrap().is_some());
    }
}
