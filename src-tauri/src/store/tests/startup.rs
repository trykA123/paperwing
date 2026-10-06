use super::{barrier, database, listing, open};
use crate::platform::Fixture;
use crate::store::error::{Error, Reason};
use crate::store::open::{open_writer, rename_group};
use crate::store::{listings, migrations::MIGRATIONS, Store};
use rusqlite::{ffi, Error as Sqlite};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn failure(code: i32) -> Sqlite {
    Sqlite::SqliteFailure(ffi::Error::new(code), None)
}

fn asides(directory: &Path) -> usize {
    std::fs::read_dir(directory)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        .count()
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "condition not reached");
        std::thread::yield_now();
    }
}

fn marker(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.running", path.display()))
}

#[test]
fn only_corruption_and_migration_failures_reset_the_store() {
    for code in [ffi::SQLITE_NOTADB, ffi::SQLITE_CORRUPT] {
        assert!(Error::Sqlite(failure(code)).resets_store());
    }
    for code in [
        ffi::SQLITE_CANTOPEN,
        ffi::SQLITE_IOERR,
        ffi::SQLITE_PERM,
        ffi::SQLITE_READONLY,
        ffi::SQLITE_BUSY,
        ffi::SQLITE_LOCKED,
        ffi::SQLITE_FULL,
    ] {
        assert!(!Error::Sqlite(failure(code)).resets_store());
        assert!(!Error::Migration(failure(code)).resets_store());
    }
    assert!(Error::Migration(failure(ffi::SQLITE_CONSTRAINT)).resets_store());
    assert!(Error::Unusable(Reason::Corrupt("damaged".into())).resets_store());
    assert!(!Error::Io(std::io::Error::other("disk")).resets_store());
    assert!(!Error::Unavailable.resets_store());
}

#[test]
fn an_unopenable_path_disables_the_store_and_keeps_the_file() {
    let fixture = Fixture::new("store-cannot-open");
    let path = database(&fixture);
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("keep"), b"data").unwrap();
    assert!(open_writer(&path, MIGRATIONS).is_err());
    assert!(path.join("keep").exists());
    assert_eq!(asides(&fixture.0), 0);
}

#[cfg(unix)]
#[test]
fn an_unreadable_database_is_kept_and_the_store_is_disabled() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("store-denied");
    let path = database(&fixture);
    drop(open(&fixture));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&path).is_ok() {
        return;
    }
    let result = open_writer(&path, MIGRATIONS);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(result.is_err());
    assert!(path.exists());
    assert_eq!(asides(&fixture.0), 0);
}

#[test]
fn sidecars_move_with_the_main_file() {
    let fixture = Fixture::new("store-aside-group");
    let path = database(&fixture);
    let aside = fixture.0.join("store.sqlite3.corrupt-1");
    for suffix in ["", "-wal", "-shm"] {
        std::fs::write(format!("{}{suffix}", path.display()), suffix).unwrap();
    }
    rename_group(&path, &aside).unwrap();
    for suffix in ["", "-wal", "-shm"] {
        assert!(!Path::new(&format!("{}{suffix}", path.display())).exists());
        let kept = std::fs::read(format!("{}{suffix}", aside.display())).unwrap();
        assert_eq!(kept, suffix.as_bytes());
    }
}

#[test]
fn a_failed_sidecar_rename_rolls_everything_back() {
    let fixture = Fixture::new("store-aside-rollback");
    let path = database(&fixture);
    let aside = fixture.0.join("store.sqlite3.corrupt-1");
    for suffix in ["", "-wal", "-shm"] {
        std::fs::write(format!("{}{suffix}", path.display()), suffix).unwrap();
    }
    let blocker = PathBuf::from(format!("{}-shm", aside.display()));
    std::fs::create_dir(&blocker).unwrap();
    std::fs::write(blocker.join("occupied"), b"x").unwrap();
    assert!(rename_group(&path, &aside).is_err());
    for suffix in ["", "-wal", "-shm"] {
        let kept = std::fs::read(format!("{}{suffix}", path.display())).unwrap();
        assert_eq!(kept, suffix.as_bytes());
    }
    assert!(!aside.exists());
    assert!(!PathBuf::from(format!("{}-wal", aside.display())).exists());
}

#[test]
fn a_pending_store_answers_every_call_as_unavailable() {
    let store = Store::pending();
    assert!(!store.is_ready());
    assert!(matches!(
        store.read_blocking(|_| Ok(())),
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        store.write_blocking(|_| Ok(())),
        Err(Error::Unavailable)
    ));
    store.post(|_| Ok(()));
    store.remove_source("source");
    store.mark_clean();
}

#[test]
fn start_returns_at_once_and_becomes_ready_in_the_background() {
    let fixture = Fixture::new("store-start");
    let store = Store::start(fixture.0.clone(), None);
    wait_until(|| store.is_ready());
    let next = listing("fresh", 1, 0, 5);
    store
        .write_blocking(move |connection| listings::put(connection, &next))
        .unwrap();
    barrier(&store);
}

#[test]
fn legacy_listing_files_are_deleted_once_at_startup() {
    let fixture = Fixture::new("store-legacy-data");
    let cache = Fixture::new("store-legacy-cache");
    for name in ["repos-a.json", "repos-b.json", "repos-c.txt", "other.json"] {
        std::fs::write(cache.0.join(name), b"[]").unwrap();
    }
    let store = Store::start(fixture.0.clone(), Some(cache.0.clone()));
    wait_until(|| !cache.0.join("repos-a.json").exists() && !cache.0.join("repos-b.json").exists());
    wait_until(|| store.is_ready());
    assert!(cache.0.join("repos-c.txt").exists());
    assert!(cache.0.join("other.json").exists());
}

#[test]
fn quick_check_runs_only_after_an_unclean_shutdown() {
    let fixture = Fixture::new("store-quick-check");
    let path = database(&fixture);
    let store = open(&fixture);
    store
        .write_blocking(|connection| listings::put(connection, &listing("one", 400, 0, 5)))
        .unwrap();
    store
        .write_blocking(|connection| {
            Ok(connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?)
        })
        .unwrap();
    store.mark_clean();
    assert!(!marker(&path).exists());
    drop(store);
    damage_second_page(&path);
    let clean = open(&fixture);
    assert!(clean.recovered_from().is_none());
    assert!(marker(&path).exists());
    drop(clean);
    let unclean = open(&fixture);
    assert!(unclean.recovered_from().is_some());
}

fn damage_second_page(path: &Path) {
    use std::io::{Seek, SeekFrom, Write};
    let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(4096)).unwrap();
    file.write_all(&[0xFF; 4096]).unwrap();
}
