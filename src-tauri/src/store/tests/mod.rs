mod concurrency;
mod crash;
mod pruning;
mod recovery;
mod schema;
mod startup;

use super::listings::{Listing, VERSION};
use super::{Options, Store};
use crate::github::Repo;
use crate::platform::Fixture;
use std::path::PathBuf;

pub(super) fn database(fixture: &Fixture) -> PathBuf {
    fixture.0.join("store.sqlite3")
}

pub(super) fn open(fixture: &Fixture) -> Store {
    Store::open(&database(fixture), &Options::default()).unwrap()
}

pub(super) fn repo(source: &str, index: usize, generation: usize) -> Repo {
    Repo {
        id: format!("{source}:owner/repo-{index}"),
        source: source.into(),
        org: "owner".into(),
        name: format!("repo-{index}"),
        description: format!("generation {generation}"),
        url: format!("git@github.com:owner/repo-{index}.git"),
        default_branch: "main".into(),
        pushed_at: String::new(),
        archived: false,
    }
}

pub(super) fn listing(source: &str, count: usize, generation: usize, fetched_at: u64) -> Listing {
    Listing {
        source_id: source.into(),
        scope: "scope".into(),
        login: Some("admin".into()),
        fetched_at,
        version: VERSION,
        repos: (0..count)
            .map(|index| repo(source, index, generation))
            .collect(),
    }
}

pub(super) fn barrier(store: &Store) {
    store.write_blocking(|_| Ok(())).unwrap();
}
