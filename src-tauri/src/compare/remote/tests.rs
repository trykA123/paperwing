use super::*;
use crate::compare::{CompareRef, CompareSource, Endpoint, RefreshResult, Service};
use crate::github::compare::fixture::{page, source, Reply, Server};
use crate::github::compare::Binding;
use crate::settings::Settings;
use std::path::PathBuf;
use std::sync::Arc;

struct Fixture {
    root: PathBuf,
    settings: Settings,
    service: Arc<Service>,
    store: crate::store::Store,
}
impl Fixture {
    fn new(source: crate::settings::Source) -> Self {
        let root = crate::test_support::tmp_root().join(format!(
            "p40-remote-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("workspace")).unwrap();
        let item = |id| serde_json::json!({"id":id,"repoId":format!("{}:admin/repo",source.id),"org":"admin","name":"repo","url":format!("https://{}/admin/repo.git",source.host),"ref":{"type":"branch","name":"main"}});
        let workspace = serde_json::json!({"root":root.join("workspace"),"layout":"flat","sets":[{"id":"set","name":"admin","items":[item("left"),item("right")]}]});
        let settings = Settings {
            sources: vec![source],
            workspace,
        };
        let service = Arc::new(Service::default());
        let store =
            crate::store::Store::open(&root.join("store.sqlite3"), &Default::default()).unwrap();
        service
            .configure_remote(root.join("cache"), store.clone())
            .unwrap();
        Self {
            root,
            settings,
            service,
            store,
        }
    }

    fn endpoint(&self, side: &str, reference: CompareRef) -> Endpoint {
        Endpoint {
            set_id: "set".into(),
            item_id: side.into(),
            reference,
        }
    }

    async fn open(&self, refs: [CompareRef; 2]) -> String {
        let [left, right] = refs;
        self.service
            .open(
                &self.settings,
                self.endpoint("left", left),
                self.endpoint("right", right),
            )
            .await
            .unwrap()
            .id
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.store.close();
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

fn reply(request: &str) -> Reply {
    let path = request.split_whitespace().nth(1).unwrap();
    if path.contains("/compare/") {
        let mut data = page(1, 1);
        data["total_commits"] = 1.into();
        data["ahead_by"] = 1.into();
        data["behind_by"] = 0.into();
        return Reply::json(data);
    }
    if path.contains("/git/commits/") {
        return Reply::json(
            serde_json::json!({"tree":{"sha": if path.ends_with(&"c".repeat(40)) { "1".repeat(40) } else { "2".repeat(40) }}}),
        );
    }
    if path.contains("/git/trees/") {
        return Reply::json(
            serde_json::json!({"truncated":false,"tree":[{"path":"file-0.txt","type":"blob","mode":"100644","size":9,"sha":if path.ends_with(&"1".repeat(40)) { "e".repeat(40) } else { "d".repeat(40) }}]}),
        );
    }
    if path.contains("/git/blobs/") {
        return Reply {
            status: 200,
            headers: String::new(),
            body: if path.ends_with(&"e".repeat(40)) {
                b"same\nold\n".to_vec()
            } else {
                b"same\nnew\n".to_vec()
            },
        };
    }
    if path.contains("/commits/heads%2F") {
        return Reply::json(
            serde_json::json!({"sha": if path.ends_with("main") { "a".repeat(40) } else { "b".repeat(40) }}),
        );
    }
    panic!("unexpected fixture route {path}");
}

fn refs() -> [CompareRef; 2] {
    [
        CompareRef::Branch {
            name: "main".into(),
        },
        CompareRef::Branch {
            name: "topic".into(),
        },
    ]
}

#[tokio::test]
async fn not_cloned_same_repository_returns_ready_snapshot_and_read_only_blob_diff_on_both_hosts() {
    for host in ["github.com", "gitext.company.com"] {
        let source = source(host);
        let server = Server::new(|request, _| reply(request)).await;
        let _binding = Binding::new(&source.id, &server.base);
        let fixture = Fixture::new(source);
        let id = fixture.open(refs()).await;
        let result = fixture
            .service
            .refresh(&fixture.settings, &id, Options::default())
            .await
            .unwrap();
        let RefreshResult::Ready { snapshot } = result else {
            panic!("remote comparison unavailable");
        };
        assert_eq!(snapshot.source, "github");
        assert_eq!(snapshot.left.commit, "a".repeat(40));
        assert_eq!(snapshot.right.commit, "b".repeat(40));
        assert!(!snapshot.truncated.files && !snapshot.truncated.commits);
        assert_eq!(server.count(), 3);
        let (prepared, job, _) = fixture
            .service
            .remote_snapshot(&fixture.settings, &id, snapshot.generation)
            .await
            .unwrap()
            .unwrap();
        let row = prepared.files(0, 10).await.remove(0);
        let cache = fixture.service.remote_cache.get().unwrap();
        for side in ["left", "right"] {
            let bytes = prepared
                .content(
                    cache,
                    ContentRequest {
                        file_id: &row.id,
                        side,
                        job: &job,
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                bytes,
                if side == "left" {
                    b"same\nold\n"
                } else {
                    b"same\nnew\n"
                }
            );
        }
        let row = prepared.files(0, 10).await.remove(0);
        assert_eq!(
            row.raw_lines,
            Some(crate::compare::Lines {
                added: 1,
                removed: 1
            })
        );
        assert_eq!(server.count(), 9);
        assert!(fixture
            .service
            .write_context(
                &fixture.settings,
                &id,
                snapshot.generation,
                &row.id,
                "left",
                false
            )
            .await
            .is_err());
        fixture.service.close(&id).await;
    }
}

#[tokio::test]
async fn disabled_provider_and_working_tree_and_unrelated_repository_never_fetch_or_cache() {
    let source = source("gitint.company.com");
    let server = Server::new(|request, _| reply(request)).await;
    let _binding = Binding::new(&source.id, &server.base);
    let mut fixture = Fixture::new(source);
    fixture.settings.sources[0].enabled = false;
    let id = fixture.open(refs()).await;
    let result = fixture
        .service
        .refresh(&fixture.settings, &id, Options::default())
        .await
        .unwrap();
    assert!(matches!(result, RefreshResult::Unavailable { .. }));
    fixture.service.close(&id).await;
    fixture.settings.sources[0].enabled = true;
    let id = fixture
        .open([
            CompareRef::WorkingTree,
            CompareRef::Branch {
                name: "topic".into(),
            },
        ])
        .await;
    assert!(matches!(
        fixture
            .service
            .refresh(&fixture.settings, &id, Options::default())
            .await
            .unwrap(),
        RefreshResult::Unavailable { .. }
    ));
    fixture.service.close(&id).await;
    fixture.settings.workspace["sets"][0]["items"][1]["url"] =
        "https://gitint.company.com/admin/other.git".into();
    let id = fixture.open(refs()).await;
    assert!(matches!(
        fixture
            .service
            .refresh_with_source(
                &fixture.settings,
                &id,
                Options::default(),
                Some(CompareSource::Github)
            )
            .await
            .unwrap(),
        RefreshResult::Unavailable { .. }
    ));
    assert_eq!(server.count(), 0);
    assert!(!fixture.root.join("cache").exists());
    fixture.service.close(&id).await;
}

#[tokio::test]
async fn changed_saved_source_and_cancelled_generation_refuse_remote_reads() {
    let source = source("github.com");
    let server = Server::new(|request, _| reply(request)).await;
    let _binding = Binding::new(&source.id, &server.base);
    let mut fixture = Fixture::new(source);
    let id = fixture.open(refs()).await;
    let RefreshResult::Ready { snapshot } = fixture
        .service
        .refresh(&fixture.settings, &id, Options::default())
        .await
        .unwrap()
    else {
        panic!();
    };
    fixture.settings.sources[0].host = "other.invalid".into();
    assert!(fixture
        .service
        .remote_snapshot(&fixture.settings, &id, snapshot.generation)
        .await
        .is_err());
    fixture.service.cancel(&id).await;
    assert!(fixture
        .service
        .remote_snapshot(&fixture.settings, &id, snapshot.generation)
        .await
        .is_err());
    assert_eq!(server.count(), 3);
}

fn immutable_refs(head: &str) -> [CompareRef; 2] {
    [
        CompareRef::Commit {
            sha: "a".repeat(40),
        },
        CompareRef::Commit { sha: head.into() },
    ]
}

async fn ready(fixture: &Fixture, id: &str) -> Box<Snapshot> {
    let RefreshResult::Ready { snapshot } = fixture
        .service
        .refresh(&fixture.settings, id, Options::default())
        .await
        .unwrap()
    else {
        panic!("comparison unavailable");
    };
    snapshot
}

#[tokio::test]
async fn immutable_comparisons_use_sqlite_across_sessions_and_restarts_without_extra_http_on_both_hosts(
) {
    for host in ["github.com", "gitint.company.com"] {
        let source = source(host);
        let server = Server::new(|request, _| reply(request)).await;
        let _binding = Binding::new(&source.id, &server.base);
        let mut fixture = Fixture::new(source);
        let id = fixture.open(immutable_refs(&"b".repeat(40))).await;
        let first = ready(&fixture, &id).await;
        assert_eq!(server.count(), 1);
        let second = ready(&fixture, &id).await;
        assert_eq!(second.left.commit, first.left.commit);
        assert_eq!(second.right.commit, first.right.commit);
        assert_eq!(server.count(), 1);
        fixture.service.close(&id).await;
        fixture.store.close();
        fixture.store =
            crate::store::Store::open(&fixture.root.join("store.sqlite3"), &Default::default())
                .unwrap();
        fixture.service = Arc::new(Service::default());
        fixture
            .service
            .configure_remote(fixture.root.join("cache"), fixture.store.clone())
            .unwrap();
        let id = fixture.open(immutable_refs(&"b".repeat(40))).await;
        assert_eq!(ready(&fixture, &id).await.source, "github");
        assert_eq!(server.count(), 1);
        fixture.service.close(&id).await;
        let rows = fixture
            .store
            .read_blocking(|connection| {
                Ok(connection.query_row::<i64, _, _>(
                    "SELECT count(*) FROM github_comparisons",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(rows, 1);
    }
}

#[tokio::test]
async fn explicit_branch_refresh_resolves_fresh_shas_and_reuses_only_immutable_pairs() {
    let source = source("github.com");
    let head = Arc::new(std::sync::Mutex::new("b".repeat(40)));
    let current = head.clone();
    let server = Server::new(move |request, _| {
        if request.contains("/commits/heads%2Ftopic") {
            return Reply::json(serde_json::json!({"sha": *current.lock().unwrap()}));
        }
        reply(request)
    })
    .await;
    let _binding = Binding::new(&source.id, &server.base);
    let fixture = Fixture::new(source);
    let id = fixture.open(refs()).await;
    assert_eq!(ready(&fixture, &id).await.right.commit, "b".repeat(40));
    assert_eq!(server.count(), 3);
    assert_eq!(ready(&fixture, &id).await.right.commit, "b".repeat(40));
    assert_eq!(server.count(), 5);
    *head.lock().unwrap() = "f".repeat(40);
    assert_eq!(ready(&fixture, &id).await.right.commit, "f".repeat(40));
    assert_eq!(server.count(), 8);
    assert_eq!(ready(&fixture, &id).await.right.commit, "f".repeat(40));
    assert_eq!(server.count(), 10);
    fixture.service.close(&id).await;
    let id = fixture.open(immutable_refs(&"f".repeat(40))).await;
    ready(&fixture, &id).await;
    assert_eq!(server.count(), 10);
    fixture.service.close(&id).await;
}

#[tokio::test]
async fn disabled_store_admission_refuses_http_and_cache_then_purges_cached_comparisons() {
    let source = source("gitext.company.com");
    let server = Server::new(|request, _| reply(request)).await;
    let _binding = Binding::new(&source.id, &server.base);
    let fixture = Fixture::new(source.clone());
    crate::store::providers::configure(&fixture.store, vec![(source.id.clone(), false)]).unwrap();
    let id = fixture.open(immutable_refs(&"b".repeat(40))).await;
    assert!(matches!(
        fixture
            .service
            .refresh(&fixture.settings, &id, Options::default())
            .await
            .unwrap(),
        RefreshResult::Unavailable { .. }
    ));
    assert_eq!(server.count(), 0);
    let count = || {
        fixture
            .store
            .read_blocking(|connection| {
                Ok(connection.query_row::<i64, _, _>(
                    "SELECT count(*) FROM github_comparisons",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap()
    };
    assert_eq!(count(), 0);
    crate::store::providers::configure(&fixture.store, vec![(source.id.clone(), true)]).unwrap();
    ready(&fixture, &id).await;
    assert_eq!(server.count(), 1);
    assert_eq!(count(), 1);
    let store = fixture.store.clone();
    let id_to_disable = source.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::store::providers::save(&store, vec![(id_to_disable, false)], Vec::new())
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(count(), 0);
    assert!(matches!(
        fixture
            .service
            .refresh(&fixture.settings, &id, Options::default())
            .await
            .unwrap(),
        RefreshResult::Unavailable { .. }
    ));
    assert_eq!(server.count(), 1);
    fixture.service.close(&id).await;
}
