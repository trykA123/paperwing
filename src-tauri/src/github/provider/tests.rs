use super::*;
use crate::core::registry::{ProviderConfig, Registry};
use crate::store::providers;
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingTransport(AtomicUsize);

impl http::GithubApi for CountingTransport {
    fn authenticated(&self) -> bool {
        false
    }
    async fn get<T: DeserializeOwned>(&self, _path: &str) -> Result<http::Page<T>, (u16, String)> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(http::Page {
            data: serde_json::from_value(serde_json::json!([{"name":"repo", "owner":{"login":"admin"}, "ssh_url":"git@enterprise.invalid:admin/repo.git"}])).unwrap(),
            next: false,
        })
    }
}

#[tokio::test]
async fn disable_restart_enable_keeps_zero_traffic_and_cache_until_listing_resumes() {
    let scratch = crate::platform::Fixture::new("provider-restart");
    let path = scratch.0.join("store.sqlite3");
    let store = Store::open(&path, &Default::default()).unwrap();
    let mut source: Source = serde_json::from_value(serde_json::json!({"id":"provider-restart","name":"admin","kind":"ghe","host":"enterprise.invalid","orgs":["admin"]})).unwrap();
    let registry = Registry::default();
    let transport = CountingTransport(AtomicUsize::new(0));
    let constructed = AtomicUsize::new(0);
    let config = |source: &Source| crate::providers::configuration(source, &source.host).unwrap();
    let factory = |key: &ProviderKey| {
        constructed.fetch_add(1, Ordering::SeqCst);
        GithubProvider::new(key.clone(), Some(store.clone()))
    };
    registry.configure(&[config(&source)], factory).unwrap();
    let lease = registry.acquire(&config(&source), factory).unwrap();
    let list = lease
        .run(lease.provider.list_with(&source, false, &transport))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(list.repos.len(), 1);
    let late = cache::ListingRequest::new(&source, false).unwrap();
    let seeding = store.clone();
    let id = source.id.clone();
    let url = list.repos[0].url.clone();
    tauri::async_runtime::spawn_blocking(move || {
        seeding.write_blocking(move |connection| {
            crate::store::commits::put(
                connection,
                &crate::store::commits::CommitSet {
                    source_id: id,
                    scope: "fixture".into(),
                    repository: "admin/repo".into(),
                    branch: "main".into(),
                    ref_epoch: 0,
                    fetched_at: 1,
                    commits: vec![Commit {
                        sha: "a".repeat(40),
                        message: "fixture".into(),
                        author: "admin".into(),
                        date: String::new(),
                        parents: vec![],
                    }],
                },
            )?;
            crate::store::refs::put(
                connection,
                &crate::store::refs::RefSet {
                    url,
                    scope: "fixture".into(),
                    ref_epoch: 0,
                    fetched_at: 1,
                    branches: vec![crate::store::refs::RefEntry {
                        name: "main".into(),
                        sha: Some("a".repeat(40)),
                        label: None,
                    }],
                    tags: vec![],
                },
            )
        })
    })
    .await
    .unwrap()
    .unwrap();
    source.enabled = false;
    let saving = store.clone();
    let id = source.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        providers::save(&saving, vec![(id, false)], Vec::new())
    })
    .await
    .unwrap()
    .unwrap();
    registry.configure(&[config(&source)], factory).unwrap();
    assert!(registry.acquire(&config(&source), factory).is_err());
    assert_eq!(transport.0.swap(0, Ordering::SeqCst), 1);
    assert!(store
        .read_blocking(|connection| crate::store::listings::get(connection, &source.id))
        .unwrap()
        .is_none());
    late.finish(store.clone(), None, &list).await.unwrap();
    let rows: i64 = store.read_blocking(|connection| Ok(connection.query_row(
        "SELECT (SELECT count(*) FROM github_listings) + (SELECT count(*) FROM repositories) + (SELECT count(*) FROM commit_sets) + (SELECT count(*) FROM commits) + (SELECT count(*) FROM ref_sets) + (SELECT count(*) FROM refs)", [], |row| row.get(0))?)).unwrap();
    assert_eq!(rows, 0);
    drop(lease);
    drop(registry);
    store.close();

    let reopened = Store::open(&path, &Default::default()).unwrap();
    assert_eq!(
        providers::flags(&reopened).unwrap().get(&source.id),
        Some(&false)
    );
    let registry = Registry::default();
    let factory = |key: &ProviderKey| {
        constructed.fetch_add(1, Ordering::SeqCst);
        GithubProvider::new(key.clone(), Some(reopened.clone()))
    };
    registry.configure(&[config(&source)], factory).unwrap();
    assert!(registry
        .acquire(
            &ProviderConfig {
                enabled: true,
                ..config(&source)
            },
            factory
        )
        .is_err());
    assert_eq!(constructed.load(Ordering::SeqCst), 1);
    assert_eq!(transport.0.load(Ordering::SeqCst), 0);
    let saving = reopened.clone();
    let id = source.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        providers::save(&saving, vec![(id, true)], Vec::new())
    })
    .await
    .unwrap()
    .unwrap();
    source.enabled = true;
    registry.configure(&[config(&source)], factory).unwrap();
    let lease = registry.acquire(&config(&source), factory).unwrap();
    assert_eq!(
        lease
            .run(lease.provider.list_with(&source, true, &transport))
            .await
            .unwrap()
            .unwrap()
            .repos
            .len(),
        1
    );
    assert_eq!(transport.0.load(Ordering::SeqCst), 1);
    assert_eq!(constructed.load(Ordering::SeqCst), 2);
    reopened.close();
}
