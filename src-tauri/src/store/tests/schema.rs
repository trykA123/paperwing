use super::{barrier, listing, open};
use crate::platform::Fixture;
use crate::store::commits::{self, CommitRow, CommitSet, Key};
use crate::store::refs::{self, RefEntry, RefSet};
use crate::store::{listings, search};

fn commit(sha: &str, message: &str) -> CommitRow {
    CommitRow {
        sha: sha.into(),
        message: message.into(),
        author: "admin".into(),
        date: "2026-10-06T00:00:00Z".into(),
        parents: vec!["parent".into()],
    }
}

fn set(commits: Vec<CommitRow>) -> CommitSet {
    CommitSet {
        source_id: "source".into(),
        scope: "scope".into(),
        repository: "owner/repo".into(),
        branch: "main".into(),
        ref_epoch: 0,
        fetched_at: 7,
        commits,
    }
}

const KEY: Key = Key {
    source_id: "source",
    repository: "owner/repo",
    branch: "main",
    ref_epoch: 0,
};

#[test]
fn listings_round_trip_in_order_and_replace_atomically() {
    let fixture = Fixture::new("store-listing");
    let store = open(&fixture);
    let first = listing("source", 3, 0, 5);
    let expected = first.clone();
    store
        .write_blocking(move |connection| listings::put(connection, &first))
        .unwrap();
    let second = listing("source", 1, 1, 6);
    store
        .write_blocking(move |connection| listings::put(connection, &second))
        .unwrap();
    let stored = store
        .read_blocking(|connection| listings::get(connection, "source"))
        .unwrap()
        .unwrap();
    assert_eq!(stored.repos.len(), 1);
    assert_eq!(stored.fetched_at, 6);
    assert_ne!(stored, expected);
}

#[test]
fn commits_round_trip_and_are_removed_with_their_source() {
    let fixture = Fixture::new("store-commits");
    let store = open(&fixture);
    let rows = vec![
        commit("a1", "Fix parser\n\nbody"),
        commit("b2", "Add cache"),
    ];
    let saved = set(rows.clone());
    store
        .write_blocking(move |connection| commits::put(connection, &saved))
        .unwrap();
    let stored = store
        .read_blocking(|connection| commits::get(connection, &KEY))
        .unwrap()
        .unwrap();
    assert_eq!(stored.commits, rows);
    store.remove_source("source");
    barrier(&store);
    assert!(store
        .read_blocking(|connection| commits::get(connection, &KEY))
        .unwrap()
        .is_none());
}

#[test]
fn refs_round_trip_with_their_epoch() {
    let fixture = Fixture::new("store-refs");
    let store = open(&fixture);
    let entry = |name: &str| RefEntry {
        name: name.into(),
        sha: Some("abc".into()),
        label: None,
    };
    let saved = RefSet {
        url: "git@github.com:owner/repo.git".into(),
        scope: "scope".into(),
        ref_epoch: 4,
        fetched_at: 9,
        branches: vec![entry("main"), entry("dev")],
        tags: vec![entry("v1")],
    };
    let expected = saved.clone();
    store
        .write_blocking(move |connection| refs::put(connection, &saved))
        .unwrap();
    let stored = store
        .read_blocking(|connection| refs::get(connection, &expected.url))
        .unwrap();
    assert_eq!(stored, Some(expected));
}

#[test]
fn fts5_finds_repository_names_and_commit_subjects() {
    let fixture = Fixture::new("store-search");
    let store = open(&fixture);
    let listed = listing("source", 3, 0, 5);
    let saved = set(vec![
        commit("a1", "Fix parser crash"),
        commit("b2", "Add cache"),
    ]);
    store
        .write_blocking(move |connection| {
            listings::put(connection, &listed)?;
            commits::put(connection, &saved)
        })
        .unwrap();
    let (repositories, subjects) = store
        .read_blocking(|connection| {
            Ok((
                search::repository_names(connection, "repo-1", 5)?,
                search::commit_subjects(connection, "pars", 5)?,
            ))
        })
        .unwrap();
    assert_eq!(repositories, ["source:owner/repo-1"]);
    assert_eq!(subjects, ["a1"]);
    let replaced = listing("source", 1, 1, 6);
    store
        .write_blocking(move |connection| listings::put(connection, &replaced))
        .unwrap();
    let stale = store
        .read_blocking(|connection| search::repository_names(connection, "repo-1", 5))
        .unwrap();
    assert!(stale.is_empty());
}
