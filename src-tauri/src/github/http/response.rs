use super::Page;
use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("GitHub rate limit reached; retry after {reset_at}")]
    RateLimited { reset_at: DateTime<Utc> },
    #[error("A pull request already exists for this branch")]
    AlreadyExists,
    #[error("{message}")]
    Http { status: u16, message: String },
    #[error("{0}")]
    Message(String),
}

impl From<(u16, String)> for Error {
    fn from((status, message): (u16, String)) -> Self {
        Self::Http { status, message }
    }
}

pub(crate) struct Response {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub next: bool,
}

impl Response {
    pub fn checked(self) -> Result<Self, Error> {
        if !(200..300).contains(&self.status) {
            return Err(self.error());
        }
        Ok(self)
    }

    pub fn decode<T: DeserializeOwned>(self) -> Result<Page<T>, Error> {
        if !(200..300).contains(&self.status) {
            return Err(self.error());
        }
        let data = serde_json::from_slice(&self.body)
            .map_err(|_| Error::Message("Unexpected response from GitHub".into()))?;
        Ok(Page {
            data,
            next: self.next,
        })
    }

    pub(super) fn check_rate_limit(&self) -> Result<(), Error> {
        let exhausted = self
            .headers
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value == "0");
        let secondary = matches!(self.status, 403 | 429)
            && (self.status == 429
                || self.headers.contains_key("retry-after")
                || String::from_utf8_lossy(&self.body)
                    .to_ascii_lowercase()
                    .contains("secondary rate limit"));
        if exhausted || secondary {
            let reset_at = self.reset_time().unwrap_or_else(|| {
                DateTime::<Utc>::from(std::time::SystemTime::now()) + chrono::Duration::minutes(1)
            });
            return Err(Error::RateLimited { reset_at });
        }
        Ok(())
    }

    fn error(&self) -> Error {
        let limited = self.status == 429
            || self.headers.contains_key("retry-after")
            || self
                .headers
                .get("x-ratelimit-remaining")
                .map(|value| value == "0")
                .unwrap_or_else(|| self.headers.contains_key("x-ratelimit-reset"));
        if matches!(self.status, 403 | 429) && limited {
            if let Some(reset_at) = self.reset_time() {
                return Error::RateLimited { reset_at };
            }
        }
        if self.status == 422 && self.already_exists() {
            return Error::AlreadyExists;
        }
        let message = match self.status {
            401 => "Token is missing or invalid; update the stored GitHub token".into(),
            403 => "GitHub access denied; check token repository permissions".into(),
            404 => {
                "GitHub repository or pull request not found; check access and the remote".into()
            }
            422 => "GitHub could not create the pull request; check the branches and title".into(),
            429 => "GitHub rate limit reached; try again later".into(),
            status => format!("GitHub request failed (HTTP {status}); try again later"),
        };
        Error::Http {
            status: self.status,
            message,
        }
    }

    fn reset_time(&self) -> Option<DateTime<Utc>> {
        let header = |name| self.headers.get(name)?.to_str().ok();
        if let Some(retry) = header("retry-after") {
            if let Ok(seconds) = retry.parse::<i64>() {
                if seconds >= 0 {
                    let date = header("date")
                        .and_then(|date| DateTime::parse_from_rfc2822(date).ok())
                        .map(|date| date.with_timezone(&Utc))
                        .unwrap_or_else(|| DateTime::<Utc>::from(std::time::SystemTime::now()));
                    return date.checked_add_signed(chrono::Duration::try_seconds(seconds)?);
                }
            }
            if let Ok(date) = DateTime::parse_from_rfc2822(retry) {
                return Some(date.with_timezone(&Utc));
            }
        }
        DateTime::from_timestamp(header("x-ratelimit-reset")?.parse().ok()?, 0)
    }

    fn already_exists(&self) -> bool {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&self.body) else {
            return false;
        };
        let exists = |message: &str| {
            message
                .to_ascii_lowercase()
                .contains("a pull request already exists")
        };
        value
            .get("message")
            .and_then(|value| value.as_str())
            .is_some_and(exists)
            || value
                .get("errors")
                .and_then(|value| value.as_array())
                .is_some_and(|errors| {
                    errors.iter().any(|error| {
                        error
                            .get("message")
                            .and_then(|value| value.as_str())
                            .is_some_and(exists)
                    })
                })
    }
}

pub(super) async fn shared_page<T: DeserializeOwned>(
    host: &str,
    status: u16,
    body: impl std::future::Future<Output = Result<Response, Error>>,
) -> Result<Page<T>, (u16, String)> {
    if !(200..300).contains(&status) {
        return Err(super::http_error(status));
    }
    let response = body.await.map_err(|error| (0, error.to_string()))?;
    let data = serde_json::from_slice(&response.body)
        .map_err(|_| (0, format!("Unexpected response from {host}")))?;
    Ok(Page {
        data,
        next: response.next,
    })
}
