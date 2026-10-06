use super::*;
use crate::platform::Fixture;
use crate::store::Options;

fn source(id: &str) -> Source {
    serde_json::from_value(serde_json::json!({"id":id,"name":"admin","kind":"github","host":"github.com","orgs":["admin"]})).unwrap()
}

fn scope() -> Scope {
    ListingRequest::new(&source("cache-fixture"), false)
        .unwrap()
        .scope
}

fn store(fixture: &Fixture) -> Store {
    Store::open(&fixture.0.join("store.sqlite3"), &Options::default()).unwrap()
}

fn repo(source_id: &str, org: &str, name: &str) -> super::super::Repo {
    serde_json::from_value(serde_json::json!({
        "id":format!("{source_id}:{org}/{name}"), "source":source_id, "org":org, "name":name,
        "description":"", "url":format!("git@github.com:{org}/{name}.git"), "defaultBranch":"main", "pushedAt":"", "archived":false
    })).unwrap()
}

fn stored(scope: &Scope, repos: Vec<super::super::Repo>, fetched_at: u64) -> Listing {
    Listing {
        source_id: scope.source_id.clone(),
        scope: scope.configuration.clone(),
        login: Some("admin".into()),
        fetched_at,
        version: listings::VERSION,
        repos,
    }
}

#[test]
fn other_versions_future_times_and_other_scopes_are_misses() {
    let scope = scope();
    let listing = stored(&scope, Vec::new(), 10);
    assert!(read_cached(listing.clone(), &scope, 10).is_some_and(|list| list.stale));
    assert!(read_cached(listing.clone(), &scope, 9).is_none());
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
        assert!(read_cached(listing.clone(), &changed, 10).is_none());
    }
    let old = Listing {
        version: listings::VERSION - 1,
        ..listing
    };
    assert!(read_cached(old, &scope, 10).is_none());
}

#[test]
fn stored_rows_cannot_leak_repositories_from_another_source_or_owner() {
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
        assert!(read_cached(stored(&scope, vec![invalid], 10), &scope, 10).is_none());
    }
}

#[tokio::test]
async fn rows_survive_a_new_process_and_are_served_stale() {
    let source = source("cache-stale-fixture");
    let fixture = Fixture::new("metadata-cache-stale");
    let list = RepoList {
        repos: vec![repo(&source.id, "admin", "repo")],
        fetched_at: now(),
        ..Default::default()
    };
    let request = ListingRequest::new(&source, false).unwrap();
    request
        .finish(store(&fixture), Some("admin".into()), &list)
        .await
        .unwrap();
    let next_process = ListingRequest::new(&source, false).unwrap();
    let stale = next_process
        .read_stale(store(&fixture))
        .await
        .unwrap()
        .unwrap();
    assert!(stale.stale);
    assert_eq!(stale.repos.len(), 1);
}

#[tokio::test]
async fn an_unavailable_store_does_not_fail_a_successful_listing() {
    let source = source("cache-io-fixture");
    let request = ListingRequest::new(&source, false).unwrap();
    let list = RepoList {
        fetched_at: now(),
        ..Default::default()
    };
    request
        .finish(Store::disabled(), None, &list)
        .await
        .unwrap();
    assert!(request
        .read_stale(Store::disabled())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn listings_with_errors_never_refill_the_store() {
    let source = source("cache-errors-fixture");
    let fixture = Fixture::new("metadata-cache-errors");
    let store = store(&fixture);
    let request = ListingRequest::new(&source, false).unwrap();
    let kept = RepoList {
        repos: vec![repo(&source.id, "admin", "kept")],
        fetched_at: now(),
        ..Default::default()
    };
    request
        .finish(store.clone(), Some("admin".into()), &kept)
        .await
        .unwrap();
    let failed = RepoList {
        errors: vec!["admin: denied".into()],
        fetched_at: now(),
        ..Default::default()
    };
    request
        .finish(store.clone(), Some("admin".into()), &failed)
        .await
        .unwrap();
    let still = request.read_stale(store.clone()).await.unwrap().unwrap();
    assert_eq!(still.repos[0].name, "kept");
    request
        .finish(store.clone(), Some("someone-else".into()), &failed)
        .await
        .unwrap();
    assert!(request.read_stale(store).await.unwrap().is_none());
}

#[tokio::test]
async fn forced_refresh_prevents_an_old_request_from_refilling_the_store() {
    let source = source("cache-force-fixture");
    let fixture = Fixture::new("metadata-cache");
    let store = store(&fixture);
    let older = ListingRequest::new(&source, false).unwrap();
    let newer = ListingRequest::new(&source, true).unwrap();
    let list = RepoList {
        repos: vec![repo(&source.id, "admin", "newer")],
        fetched_at: now(),
        ..Default::default()
    };
    newer
        .finish(store.clone(), Some("admin".into()), &list)
        .await
        .unwrap();
    let older_list = RepoList {
        repos: vec![repo(&source.id, "admin", "older")],
        ..list
    };
    assert!(older
        .finish(store.clone(), None, &older_list)
        .await
        .is_err());
    assert!(older.read_stale(store.clone()).await.is_err());
    let saved = newer.read_stale(store).await.unwrap().unwrap();
    assert_eq!(saved.repos[0].name, "newer");
}

#[tokio::test]
async fn no_token_or_credential_state_reaches_the_database() {
    let token = "ghp_SkeinSentinelToken0123456789";
    let source = source("cache-privacy-fixture");
    let fixture = Fixture::new("metadata-cache-privacy");
    let store = store(&fixture);
    let request = ListingRequest::new(&source, false).unwrap();
    let list = RepoList {
        repos: vec![repo(&source.id, "admin", "repo")],
        fetched_at: now(),
        ..Default::default()
    };
    request
        .finish(store.clone(), Some("admin".into()), &list)
        .await
        .unwrap();
    let columns: Vec<String> = tauri::async_runtime::spawn_blocking(move || {
        store
            .write_blocking(|connection| {
                connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
                Ok(())
            })
            .unwrap();
        store
            .read_blocking(|connection| {
                let mut statement = connection.prepare(
                "SELECT m.name || '.' || p.name FROM sqlite_master m, pragma_table_info(m.name) p
                 WHERE m.type = 'table'",
            )?;
                let rows = statement.query_map([], |row| row.get(0))?;
                Ok(rows.collect::<Result<_, _>>()?)
            })
            .unwrap()
    })
    .await
    .unwrap();
    let forbidden = [
        "token",
        "secret",
        "credential",
        "password",
        "authorization",
        "revision",
    ];
    for column in &columns {
        let lower = column.to_lowercase();
        assert!(
            !forbidden.iter().any(|word| lower.contains(word)),
            "column {column}"
        );
    }
    for entry in std::fs::read_dir(&fixture.0).unwrap().flatten() {
        let bytes = std::fs::read(entry.path()).unwrap_or_default();
        assert!(!bytes
            .windows(token.len())
            .any(|window| window == token.as_bytes()));
    }
}
