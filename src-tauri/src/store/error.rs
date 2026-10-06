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

impl Error {
    pub(super) fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Sqlite(rusqlite::Error::SqliteFailure(failure, _))
                if matches!(
                    failure.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                )
        )
    }
}

impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}
