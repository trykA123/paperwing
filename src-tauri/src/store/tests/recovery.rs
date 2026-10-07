use super::{barrier, database, listing, open};
use crate::platform::Fixture;
use crate::store::migrations::{self, Migration, MIGRATIONS};
use crate::store::open::open_writer;
use crate::store::{listings, Options, Store};
use rusqlite::Connection;
use std::path::PathBuf;

const FIRST: Migration = Migration {
    version: 1,
    sql: "CREATE TABLE sample (value TEXT NOT NULL);",
};
const SECOND: Migration = Migration {
    version: 2,
    sql: "CREATE UNIQUE INDEX sample_value ON sample (value);",
};
const BOTH: &[Migration] = &[FIRST, SECOND];

fn asides(fixture: &Fixture) -> Vec<PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(&fixture.0)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            name.contains(".corrupt-") && !name.ends_with("-wal") && !name.ends_with("-shm")
        })
        .collect();
    found.sort();
    found
}

fn pragma(store: &Store, name: &'static str) -> String {
    store
        .write_blocking(move |connection| {
            Ok(connection.query_row(&format!("PRAGMA {name}"), [], |row| {
                row.get::<_, rusqlite::types::Value>(0)
            })?)
        })
        .map(|value| match value {
            rusqlite::types::Value::Integer(number) => number.to_string(),
            rusqlite::types::Value::Text(text) => text,
            other => format!("{other:?}"),
        })
        .unwrap()
}

#[test]
fn an_empty_directory_migrates_to_the_latest_schema() {
    let fixture = Fixture::new("store-empty");
    let store = open(&fixture);
    assert!(store.recovered_from().is_none());
    assert_eq!(pragma(&store, "user_version"), "2");
    assert_eq!(pragma(&store, "journal_mode"), "wal");
    assert_eq!(pragma(&store, "foreign_keys"), "1");
    assert_eq!(pragma(&store, "auto_vacuum"), "2");
    let tables: i64 = store
        .read_blocking(|connection| {
            Ok(connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name IN
                 ('github_listings','repositories','commit_sets','commits','ref_sets','refs',
                  'repository_search','commit_search')",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(tables, 8);
}

#[test]
fn a_corrupt_file_is_renamed_aside_and_the_store_starts_empty() {
    let fixture = Fixture::new("store-corrupt");
    let path = database(&fixture);
    std::fs::write(&path, b"this is not a sqlite database, only text").unwrap();
    let store = open(&fixture);
    let aside = store.recovered_from().expect("the file was moved aside");
    assert_eq!(
        std::fs::read(aside).unwrap(),
        b"this is not a sqlite database, only text"
    );
    let next = listing("fresh", 2, 0, 5);
    store
        .write_blocking(move |connection| listings::put(connection, &next))
        .unwrap();
    barrier(&store);
    let stored = store
        .read_blocking(|connection| listings::get(connection, "fresh"))
        .unwrap();
    assert_eq!(stored.unwrap().repos.len(), 2);
}

#[test]
fn a_failed_migration_renames_the_database_aside_and_starts_empty() {
    let fixture = Fixture::new("store-migration");
    let path = database(&fixture);
    {
        let mut old = Connection::open(&path).unwrap();
        migrations::apply(&mut old, &[FIRST]).unwrap();
        old.execute_batch("INSERT INTO sample VALUES ('same'), ('same');")
            .unwrap();
    }
    let options = Options {
        migrations: BOTH,
        ..Options::default()
    };
    let store = Store::open(&path, &options).unwrap();
    let aside = store.recovered_from().expect("the old file was kept");
    let kept = Connection::open(aside).unwrap();
    let rows: i64 = kept
        .query_row("SELECT count(*) FROM sample", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 2);
    assert_eq!(pragma(&store, "user_version"), "2");
    let fresh: i64 = store
        .read_blocking(|connection| {
            Ok(connection.query_row("SELECT count(*) FROM sample", [], |row| row.get(0))?)
        })
        .unwrap();
    assert_eq!(fresh, 0);
}

#[test]
fn a_database_from_a_newer_build_is_kept_aside() {
    let fixture = Fixture::new("store-newer");
    let path = database(&fixture);
    {
        let mut newer = Connection::open(&path).unwrap();
        migrations::apply(&mut newer, BOTH).unwrap();
    }
    let options = Options {
        migrations: &[FIRST],
        ..Options::default()
    };
    let store = Store::open(&path, &options).unwrap();
    assert!(store.recovered_from().is_some());
    assert_eq!(pragma(&store, "user_version"), "1");
}

#[test]
fn only_the_newest_moved_aside_files_are_kept() {
    let fixture = Fixture::new("store-asides");
    let path = database(&fixture);
    for _ in 0..5 {
        std::fs::write(&path, b"garbage").unwrap();
        let opened = open_writer(&path, MIGRATIONS).unwrap();
        assert!(opened.recovered_from.is_some());
        drop(opened);
        std::fs::remove_file(&path).unwrap();
        for sidecar in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{sidecar}", path.display()));
        }
    }
    assert_eq!(asides(&fixture).len(), 3);
}
