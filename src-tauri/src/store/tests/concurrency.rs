use super::{listing, open};
use crate::platform::Fixture;
use crate::store::listings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn readers_see_whole_listings_while_one_writer_replaces_them() {
    let fixture = Fixture::new("store-concurrent");
    let store = open(&fixture);
    let done = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..6)
        .map(|_| {
            let (store, done) = (store.clone(), done.clone());
            std::thread::spawn(move || {
                let mut reads = 0;
                while !done.load(Ordering::Acquire) {
                    let stored = store
                        .read_blocking(|connection| listings::get(connection, "shared"))
                        .unwrap();
                    if let Some(stored) = stored {
                        assert_eq!(stored.repos.len(), 60);
                        let first = &stored.repos[0].description;
                        assert!(stored.repos.iter().all(|repo| &repo.description == first));
                        reads += 1;
                    }
                }
                reads
            })
        })
        .collect();
    for generation in 0..150 {
        let next = listing("shared", 60, generation, 100 + generation as u64);
        store
            .write_blocking(move |connection| listings::put(connection, &next))
            .unwrap();
    }
    done.store(true, Ordering::Release);
    let reads: usize = readers
        .into_iter()
        .map(|reader| reader.join().unwrap())
        .sum();
    assert!(reads > 0);
}
