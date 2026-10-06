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

pub(super) struct Opened {
    pub connection: Connection,
    pub recovered_from: Option<PathBuf>,
}

pub(super) fn open_writer(path: &Path, migrations: &[Migration]) -> Result<Opened, Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match prepare(path, migrations) {
        Ok(connection) => Ok(Opened {
            connection,
            recovered_from: None,
        }),
        Err(error) if error.is_busy() || matches!(error, Error::Io(_)) => Err(error),
        Err(error) => {
            eprintln!("Local store reset: {error}");
            let aside = move_aside(path)?;
            Ok(Opened {
                connection: prepare(path, migrations)?,
                recovered_from: Some(aside),
            })
        }
    }
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

fn prepare(path: &Path, migrations: &[Migration]) -> Result<Connection, Error> {
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
    let check: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if check != "ok" {
        return Err(Error::Unusable(Reason::Corrupt(check)));
    }
    migrations::apply(&mut connection, migrations)?;
    Ok(connection)
}

fn move_aside(path: &Path) -> Result<PathBuf, Error> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let aside = suffixed(path, &format!(".corrupt-{stamp}"));
    std::fs::rename(path, &aside)?;
    for extension in ["-wal", "-shm"] {
        let sidecar = suffixed(path, extension);
        if sidecar.exists() {
            std::fs::rename(&sidecar, suffixed(&aside, extension))?;
        }
    }
    forget_old_asides(path);
    Ok(aside)
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
