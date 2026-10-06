use super::*;
use crate::platform::Fixture;
use crate::store::Options;

fn source(id: &str, owner: &str) -> Source {
    serde_json::from_value(serde_json::json!({"id":id,"name":"admin","kind":"github","host":"github.com","orgs":[owner]})).unwrap()
}

fn commit(sha: &str) -> Commit {
    Commit {
        sha: sha.into(),
        message: "Subject\n\nBody".into(),
        author: "admin".into(),
        date: "2026-10-06T00:00:00Z".into(),
        parents: vec!["parent".into()],
    }
}

fn store(fixture: &Fixture) -> Store {
    Store::open(&fixture.0.join("store.sqlite3"), &Options::default()).unwrap()
}

fn request(source: &Source, revision: u64) -> Request<'_> {
    Request {
        source,
        org: "admin",
        name: "repo",
        branch: "main",
        revision,
    }
}

fn settle(store: &Store) {
    store.write_blocking(|_| Ok(())).unwrap();
}

#[test]
fn commits_are_recalled_for_the_same_source_configuration_only() {
    let fixture = Fixture::new("commit-cache");
    let store = store(&fixture);
    let saved = source("commit-cache-fixture", "admin");
    let revision = crate::credentials::metadata_revision(&saved).unwrap();
    remember(&store, &request(&saved, revision), &[commit("a1")]);
    settle(&store);
    let recalled = recall(&store, &request(&saved, revision)).unwrap();
    assert_eq!(recalled, [commit("a1")]);
    let edited = source("commit-cache-fixture", "another-owner");
    assert!(recall(&store, &request(&edited, revision)).is_none());
}

#[test]
fn a_changed_credential_revision_prevents_publication() {
    let fixture = Fixture::new("commit-cache-stale");
    let store = store(&fixture);
    let saved = source("commit-cache-revision-fixture", "admin");
    let revision = crate::credentials::metadata_revision(&saved).unwrap();
    remember(&store, &request(&saved, revision + 1), &[commit("a1")]);
    settle(&store);
    assert!(recall(&store, &request(&saved, revision)).is_none());
}
