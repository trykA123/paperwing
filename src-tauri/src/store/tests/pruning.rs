use super::{barrier, listing};
use crate::platform::Fixture;
use crate::store::{listings, size, Options, Store};

const CAP: u64 = 512 * 1024;

#[test]
fn size_cap_prunes_the_oldest_fetched_entries_first() {
    let fixture = Fixture::new("store-prune");
    let options = Options {
        max_bytes: CAP,
        ..Options::default()
    };
    let store = Store::open(&super::database(&fixture), &options).unwrap();
    for index in 0..40 {
        let next = listing(&format!("source-{index}"), 150, 0, 1_000 + index as u64);
        store
            .write_blocking(move |connection| listings::put(connection, &next))
            .unwrap();
    }
    barrier(&store);
    let used = store.read_blocking(size::used_bytes).unwrap();
    assert!(used <= CAP, "used {used} bytes");
    let present = |source: &str| {
        store
            .read_blocking(|connection| listings::get(connection, source))
            .unwrap()
            .is_some()
    };
    assert!(!present("source-0"));
    assert!(present("source-39"));
}

#[test]
fn pruned_repositories_leave_no_search_rows() {
    let fixture = Fixture::new("store-prune-search");
    let store = super::open(&fixture);
    let next = listing("gone", 5, 0, 1);
    store
        .write_blocking(move |connection| listings::put(connection, &next))
        .unwrap();
    store
        .write_blocking(|connection| size::enforce(connection, 0))
        .unwrap();
    let hits = store
        .read_blocking(|connection| crate::store::search::repository_names(connection, "repo", 10))
        .unwrap();
    assert!(hits.is_empty());
}
