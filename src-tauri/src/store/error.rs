use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("local store is unavailable")]
    Unavailable,
    #[error("local store database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("local store file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("local store file is not usable: {0}")]
    Unusable(Reason),
    #[error("local store migration failed: {0}")]
    Migration(rusqlite::Error),
}

#[derive(Debug)]
pub enum Reason {
    Corrupt(String),
    NewerSchema { found: u32, supported: u32 },
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt(detail) => write!(formatter, "integrity check failed ({detail})"),
            Self::NewerSchema { found, supported } => {
                write!(
                    formatter,
                    "schema version {found} is newer than {supported}"
                )
            }
        }
    }
}

fn failure_code(error: &rusqlite::Error) -> Option<rusqlite::ErrorCode> {
    match error {
        rusqlite::Error::SqliteFailure(failure, _) => Some(failure.code),
        _ => None,
    }
}

fn is_environmental(code: Option<rusqlite::ErrorCode>) -> bool {
    use rusqlite::ErrorCode::{
        CannotOpen, DatabaseBusy, DatabaseLocked, DiskFull, PermissionDenied, ReadOnly,
        SystemIoFailure,
    };
    matches!(
        code,
        Some(
            CannotOpen
                | SystemIoFailure
                | PermissionDenied
                | ReadOnly
                | DatabaseBusy
                | DatabaseLocked
                | DiskFull
        )
    )
}

impl Error {
    pub(super) fn resets_store(&self) -> bool {
        use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
        match self {
            Self::Unusable(_) => true,
            Self::Sqlite(error) => {
                matches!(failure_code(error), Some(NotADatabase | DatabaseCorrupt))
            }
            Self::Migration(error) => !is_environmental(failure_code(error)),
            Self::Unavailable | Self::Io(_) => false,
        }
    }
}

impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}
