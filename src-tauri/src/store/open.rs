use super::error::{Error, Reason};
use super::migrations::{self, Migration};
use rusqlite::{Connection, OpenFlags};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const WAL_LIMIT_BYTES: i64 = 4 * 1024 * 1024;
const KEPT_ASIDE: usize = 3;
const INCREMENTAL_VACUUM: i64 = 2;
const RUNNING_SUFFIX: &str = ".running";

pub(super) struct Opened {
    pub connection: Connection,
    pub recovered_from: Option<PathBuf>,
}

pub(super) fn open_writer(path: &Path, migrations: &[Migration]) -> Result<Opened, Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let check = ran_uncleanly(path);
    let opened = match prepare(path, migrations, check) {
        Ok(connection) => Opened {
            connection,
            recovered_from: None,
        },
        Err(error) if error.resets_store() => {
            eprintln!("Local store reset: {error}");
            let aside = move_aside(path)?;
            Opened {
                connection: prepare(path, migrations, false)?,
                recovered_from: Some(aside),
            }
        }
        Err(error) => return Err(error),
    };
    if let Err(error) = std::fs::write(suffixed(path, RUNNING_SUFFIX), b"") {
        eprintln!("Local store run marker not written: {error}");
    }
    Ok(opened)
}

pub(super) fn mark_clean(path: &Path) {
    let _ = std::fs::remove_file(suffixed(path, RUNNING_SUFFIX));
}

fn ran_uncleanly(path: &Path) -> bool {
    suffixed(path, RUNNING_SUFFIX).exists()
}

pub(super) fn open_reader(path: &Path) -> Result<Connection, Error> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "query_only", true)?;
    Ok(connection)
}

fn prepare(path: &Path, migrations: &[Migration], check: bool) -> Result<Connection, Error> {
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "auto_vacuum", INCREMENTAL_VACUUM)?;
    let mode: String = connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(Error::Unusable(Reason::Corrupt(format!(
            "journal mode {mode}"
        ))));
    }
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "trusted_schema", false)?;
    connection.pragma_update(None, "journal_size_limit", WAL_LIMIT_BYTES)?;
    if check {
        let verdict: String =
            connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if verdict != "ok" {
            return Err(Error::Unusable(Reason::Corrupt(verdict)));
        }
    }
    migrations::apply(&mut connection, migrations)?;
    Ok(connection)
}

fn move_aside(path: &Path) -> Result<PathBuf, Error> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let aside = suffixed(path, &format!(".corrupt-{stamp}"));
    rename_group(path, &aside)?;
    forget_old_asides(path);
    Ok(aside)
}

pub(super) fn rename_group(path: &Path, aside: &Path) -> Result<(), Error> {
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let members = ["-wal", "-shm", ""]
        .map(|extension| (suffixed(path, extension), suffixed(aside, extension)));
    for (from, to) in members {
        if !from.exists() {
            continue;
        }
        if let Err(error) = std::fs::rename(&from, &to) {
            for (original, renamed) in moved.into_iter().rev() {
                let _ = std::fs::rename(renamed, original);
            }
            return Err(error.into());
        }
        moved.push((from, to));
    }
    Ok(())
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn forget_old_asides(path: &Path) {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let prefix = format!("{}.corrupt-", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut asides: Vec<_> = entries
        .flatten()
        .filter(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            file.starts_with(&prefix) && !file.ends_with("-wal") && !file.ends_with("-shm")
        })
        .map(|entry| entry.path())
        .collect();
    asides.sort();
    let surplus = asides.len().saturating_sub(KEPT_ASIDE);
    for old in asides.into_iter().take(surplus) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(suffixed(&old, suffix));
        }
    }
}
