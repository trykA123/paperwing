use super::{barrier, database, listing, open};
use crate::platform::Fixture;
use crate::store::listings;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const CHILD_ENV: &str = "SKEIN_STORE_CRASH_CHILD";
const REPOS: usize = 400;

#[test]
fn child_writer() {
    let Ok(path) = std::env::var(CHILD_ENV) else {
        return;
    };
    let store = crate::store::Store::open(path.as_ref(), &Default::default()).unwrap();
    let mut generation = 0;
    loop {
        let next = listing("crash", REPOS, generation, 10 + generation as u64);
        store
            .write_blocking(move |connection| listings::put(connection, &next))
            .unwrap();
        println!("generation {generation}");
        generation += 1;
    }
}

#[test]
fn sigkill_during_writes_leaves_a_consistent_store() {
    let fixture = Fixture::new("store-crash");
    let path = database(&fixture);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "store::tests::crash::child_writer",
            "--nocapture",
        ])
        .env(CHILD_ENV, &path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let seen = lines
        .map_while(Result::ok)
        .take_while(|line| line != "generation 5")
        .count();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(seen >= 1, "the child never started writing");

    let store = open(&fixture);
    assert!(store.recovered_from().is_none());
    barrier(&store);
    let stored = store
        .read_blocking(|connection| listings::get(connection, "crash"))
        .unwrap()
        .expect("a committed listing survives the kill");
    assert_eq!(stored.repos.len(), REPOS);
    let generation = &stored.repos[0].description;
    assert!(stored
        .repos
        .iter()
        .all(|repo| &repo.description == generation));
}
