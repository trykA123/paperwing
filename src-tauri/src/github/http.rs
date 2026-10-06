use super::{api_base, Source};
use serde::de::DeserializeOwned;
use std::time::Duration;

pub(super) struct Page<T> {
    pub data: T,
    pub next: bool,
}

pub(super) trait GithubApi {
    fn authenticated(&self) -> bool;
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, (u16, String)>;
}

pub(super) struct Http<'a> {
    source: &'a Source,
    base: String,
    token: Option<String>,
    revision: u64,
    client: reqwest::Client,
}

struct Connection<'a> {
    source: &'a Source,
    base: String,
    expected: u64,
}

impl<'a> Http<'a> {
    pub async fn connect(source: &'a Source) -> Result<Self, (u16, String)> {
        Self::connect_at(source, crate::credentials::revision(&source.id)).await
    }

    pub async fn connect_at(source: &'a Source, expected: u64) -> Result<Self, (u16, String)> {
        let base = api_base(&source.host).map_err(|reason| (0, reason))?;
        Self::with_token(
            Connection {
                source,
                base,
                expected,
            },
            crate::credentials::read(source.id.clone()),
            || crate::credentials::revision(&source.id),
        )
        .await
    }

    async fn with_token(
        connection: Connection<'a>,
        token: impl std::future::Future<Output = Result<Option<String>, String>>,
        revision: impl Fn() -> u64,
    ) -> Result<Self, (u16, String)> {
        if revision() != connection.expected {
            return Err(changed());
        }
        let token = token.await.map_err(|reason| (0, reason))?;
        if revision() != connection.expected {
            return Err(changed());
        }
        let client = reqwest::Client::builder()
            .user_agent("paperwing")
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| (0, "Cannot initialize GitHub HTTP client".into()))?;
        Ok(Self {
            source: connection.source,
            base: connection.base,
            token,
            revision: connection.expected,
            client,
        })
    }

    fn check_revision(&self) -> Result<(), (u16, String)> {
        if crate::credentials::revision(&self.source.id) != self.revision {
            return Err(changed());
        }
        Ok(())
    }
}

fn changed() -> (u16, String) {
    (0, "Source credentials changed; retry the request".into())
}

impl GithubApi for Http<'_> {
    fn authenticated(&self) -> bool {
        self.token.is_some()
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, (u16, String)> {
        self.check_revision()?;
        let url = format!("{}{path}", self.base);
        let mut request = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| (0, format!("Cannot reach {}", self.source.host)))?;
        self.check_revision()?;
        if !response.status().is_success() {
            return Err(http_error(response.status().as_u16()));
        }
        let next = next_page(
            &url,
            response
                .headers()
                .get("link")
                .and_then(|value| value.to_str().ok()),
        )?;
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| (0, "Cannot read GitHub response".into()))?
        {
            if body.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err((0, "GitHub response exceeds metadata limit".into()));
            }
            body.extend_from_slice(&chunk);
        }
        let data = serde_json::from_slice(&body)
            .map_err(|_| (0, format!("Unexpected response from {}", self.source.host)))?;
        self.check_revision()?;
        Ok(Page { data, next })
    }
}

fn http_error(status: u16) -> (u16, String) {
    let reason = match status {
        401 => "Token is missing or invalid".into(),
        403 => "Access denied or rate limited; check token repository permissions".into(),
        404 => "Not found".into(),
        status => format!("HTTP {status}"),
    };
    (status, reason)
}

fn next_page(current: &str, link: Option<&str>) -> Result<bool, (u16, String)> {
    let Some(next) =
        link.and_then(|link| link.split(',').find(|part| part.contains("rel=\"next\"")))
    else {
        return Ok(false);
    };
    let invalid = || (0, "Invalid GitHub pagination continuation".to_string());
    let target = next.trim().split(';').next().ok_or_else(invalid)?.trim();
    let target = target
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .ok_or_else(invalid)?;
    let current = reqwest::Url::parse(current).map_err(|_| invalid())?;
    let target = reqwest::Url::parse(target).map_err(|_| invalid())?;
    if current.origin() != target.origin()
        || current.path() != target.path()
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err(invalid());
    }
    let page = |url: &reqwest::Url| {
        url.query_pairs()
            .find(|(key, _)| key == "page")
            .and_then(|(_, value)| value.parse::<u32>().ok())
    };
    if page(&current).and_then(|page| page.checked_add(1)) != page(&target) {
        return Err(invalid());
    }
    Ok(true)
}

pub(super) async fn get_json<T: DeserializeOwned>(
    source: &Source,
    path: &str,
) -> Result<T, (u16, String)> {
    Ok(Http::connect(source).await?.get(path).await?.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn deferred_credentials_reject_changes_before_http() {
        let source: Source = serde_json::from_value(serde_json::json!({"id":"deferred-source","name":"admin","kind":"ghe","host":"old.invalid"})).unwrap();
        let revision = Arc::new(AtomicU64::new(0));
        let current = revision.clone();
        let token = async {
            current.fetch_add(1, Ordering::SeqCst);
            Ok(Some("synthetic-token".into()))
        };
        let result = Http::with_token(
            Connection {
                source: &source,
                base: "http://127.0.0.1:41020".into(),
                expected: 0,
            },
            token,
            || revision.load(Ordering::SeqCst),
        )
        .await;
        assert!(result.err().unwrap().1.contains("changed"));
        for reason in ["locked", "unavailable", "permission denied"] {
            let result = Http::with_token(
                Connection {
                    source: &source,
                    base: "http://127.0.0.1:41020".into(),
                    expected: 0,
                },
                async { Err(reason.into()) },
                || 0,
            )
            .await;
            assert_eq!(result.err().unwrap().1, reason);
        }
    }

    #[test]
    fn pagination_rejects_another_origin_endpoint_or_nonsequential_page() {
        let current = "https://enterprise.invalid/api/v3/user/repos?page=1";
        assert!(next_page(
            current,
            Some("<https://enterprise.invalid/api/v3/user/repos?page=2>; rel=\"next\"")
        )
        .unwrap());
        for target in [
            "https://other.invalid/api/v3/user/repos?page=2",
            "https://enterprise.invalid/api/v3/users/admin/repos?page=2",
            "https://enterprise.invalid/api/v3/user/repos?page=4",
        ] {
            assert!(next_page(current, Some(&format!("<{target}>; rel=\"next\""))).is_err());
        }
    }

    #[tokio::test]
    async fn obsolete_admission_does_not_acquire_replacement_credentials() {
        let source: Source = serde_json::from_value(serde_json::json!({"id":"admission-fixture","name":"admin","kind":"github","host":"github.com"})).unwrap();
        let connection = Connection {
            source: &source,
            base: "http://127.0.0.1:41020".into(),
            expected: 0,
        };
        let result = Http::with_token(
            connection,
            async { panic!("obsolete credential lookup must not start") },
            || 1,
        )
        .await;
        assert!(result.err().unwrap().1.contains("changed"));
    }
}
