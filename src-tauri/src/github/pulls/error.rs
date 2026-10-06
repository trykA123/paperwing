use super::super::http::Error;
use serde::Serialize;

#[derive(Debug, Serialize, thiserror::Error)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PullsError {
    #[error("{message}")]
    RateLimited { reset_at: String, message: String },
    #[error("{message}")]
    Message { message: String },
}

impl From<Error> for PullsError {
    fn from(error: Error) -> Self {
        match error {
            Error::RateLimited { reset_at } => Self::RateLimited {
                message: format!("GitHub rate limit reached; retry after {reset_at}"),
                reset_at: reset_at.to_rfc3339(),
            },
            error => Self::Message {
                message: error.to_string(),
            },
        }
    }
}

impl From<String> for PullsError {
    fn from(message: String) -> Self {
        Self::Message { message }
    }
}

impl From<(u16, String)> for PullsError {
    fn from((_, message): (u16, String)) -> Self {
        Self::Message { message }
    }
}
