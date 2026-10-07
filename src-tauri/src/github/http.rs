use super::{api_base, Source};
use reqwest::Method;

mod response;
pub(super) use response::{Error, Response};
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicBool, Ordering};
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
    api_host: String,
    version_header: AtomicBool,
}

struct Connection<'a> {
    source: &'a Source,
    base: String,
    host: &'a str,
    expected: u64,
}

impl<'a> Http<'a> {
    pub async fn draft(source: &'a Source, token: Option<String>) -> Result<Self, (u16, String)> {
        if let Some(token) = token.filter(|token| !token.trim().is_empty()) {
            return Self::with_token(
                Connection { source, base: api_base(&source.host).map_err(|reason| (0, reason))?, host: &source.host, expected: crate::credentials::revision(&source.id) },
                async { Ok(Some(token.trim().into())) },
                || crate::credentials::revision(&source.id),
            ).await;
        }
        crate::credentials::check_saved_host(&source.id, &source.host).map_err(|reason| (0, reason))?;
        Self::connect_at(source, crate::credentials::revision(&source.id)).await
    }

    pub async fn connect_at(source: &'a Source, expected: u64) -> Result<Self, (u16, String)> {
        Self::connect_host_at(source, expected, &source.host).await
    }

    pub async fn connect_host_at(
        source: &'a Source,
        expected: u64,
        host: &'a str,
    ) -> Result<Self, (u16, String)> {
        let base = api_base(host).map_err(|reason| (0, reason))?;
        Self::with_token(
            Connection {
                source,
                base,
                host,
                expected,
            },
            crate::credentials::read_for_host(source.id.clone(), Some(host.into())),
            || crate::credentials::revision(&source.id),
        )
        .await
    }

    async fn with_token(
        connection: Connection<'a>,
        token: impl std::future::Future<Output = Result<Option<String>, String>>,
        revision: impl Fn() -> u64,
    ) -> Result<Self, (u16, String)> {
        crate::providers::ensure_enabled(connection.source).map_err(|reason| (0, reason))?;
        if revision() != connection.expected {
            return Err(changed());
        }
        let token = if token_allowed(connection.source, connection.host) {
            token.await.map_err(|reason| (0, reason))?
        } else {
            None
        };
        if revision() != connection.expected {
            return Err(changed());
        }
        let client = reqwest::Client::builder()
            .user_agent("skein")
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
            api_host: connection.host.into(),
            version_header: AtomicBool::new(true),
        })
    }

    fn check_revision(&self) -> Result<(), (u16, String)> {
        crate::providers::ensure_enabled(self.source).map_err(|reason| (0, reason))?;
        if crate::credentials::revision(&self.source.id) != self.revision {
            return Err(changed());
        }
        Ok(())
    }
}

fn token_allowed(source: &Source, host: &str) -> bool {
    if source.kind == "manual" {
        return !source.urls.is_empty()
            && source
                .urls
                .iter()
                .all(|url| super::pulls::repository::parse_remote(url, host).is_ok());
    }
    source.host.eq_ignore_ascii_case(host)
}

fn changed() -> (u16, String) {
    (0, "Source credentials changed; retry the request".into())
}

impl Http<'_> {
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Response, Error> {
        let url = format!("{}{path}", self.base);
        let response = self.dispatch(method, path, body).await?;
        let response = self.read_response(&url, response).await?;
        self.check_revision()?;
        Ok(response)
    }

    async fn dispatch(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, Error> {
        let request = self.build_request(method, path, body)?;
        let lease = crate::providers::acquire(self.source, &self.api_host, None).map_err(Error::Message)?;
        lease.run(self.dispatch_request(request, |request| async {
            let response = self
                .client
                .execute(request)
                .await
                .map_err(|_| self.connection_error())?;
            Ok((response.status().as_u16(), response))
        }))
        .await.map_err(|error| Error::Message(error.to_string()))?
    }

    fn connection_error(&self) -> Error {
        Error::Message(format!("Cannot reach {}", self.source.host))
    }

    fn build_request(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Request, Error> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .header("Accept", "application/vnd.github+json");
        if self.version_header.load(Ordering::Relaxed) {
            request = request.header("X-GitHub-Api-Version", "2022-11-28");
        }
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.build().map_err(|_| self.connection_error())
    }

    async fn dispatch_request<T, F, Fut>(
        &self,
        request: reqwest::Request,
        send: F,
    ) -> Result<T, Error>
    where
        F: Fn(reqwest::Request) -> Fut,
        Fut: std::future::Future<Output = Result<(u16, T), Error>>,
    {
        self.check_revision()?;
        let retry = if !self.api_host.eq_ignore_ascii_case("github.com")
            && request.headers().contains_key("X-GitHub-Api-Version")
        {
            request.try_clone()
        } else {
            None
        };
        let (status, response) = send(request).await?;
        self.check_revision()?;
        if status != 400 {
            return Ok(response);
        }
        let Some(mut retry) = retry else {
            return Ok(response);
        };
        retry.headers_mut().remove("X-GitHub-Api-Version");
        self.version_header.store(false, Ordering::Relaxed);
        self.check_revision()?;
        let (_, response) = send(retry).await?;
        self.check_revision()?;
        Ok(response)
    }

    async fn read_response(
        &self,
        url: &str,
        mut response: reqwest::Response,
    ) -> Result<Response, Error> {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let next = if response.status().is_success() {
            next_page(
                url,
                headers.get("link").and_then(|value| value.to_str().ok()),
            )?
        } else {
            false
        };
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Message("Cannot read GitHub response".into()))?
        {
            if body.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(Error::Message(
                    "GitHub response exceeds metadata limit".into(),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Response {
            status,
            headers,
            body,
            next,
        })
    }
}

impl GithubApi for Http<'_> {
    fn authenticated(&self) -> bool {
        self.token.is_some()
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, (u16, String)> {
        let response = self
            .dispatch(Method::GET, path, None)
            .await
            .map_err(|error| (0, error.to_string()))?;
        let status = response.status().as_u16();
        let page = response::shared_page(
            &self.source.host,
            status,
            self.read_response(&format!("{}{path}", self.base), response),
        )
        .await?;
        self.check_revision()?;
        Ok(page)
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
        || !same_endpoint(current.path(), target.path())
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

fn same_endpoint(current: &str, target: &str) -> bool {
    if current == target {
        return true;
    }
    let current: Vec<_> = current.split('/').collect();
    let target: Vec<_> = target.split('/').collect();
    if current.len() != target.len() {
        return false;
    }
    let Some(index) = current
        .iter()
        .position(|segment| matches!(*segment, "orgs" | "users"))
    else {
        return false;
    };
    let canonical = if current[index] == "orgs" {
        "organizations"
    } else {
        "user"
    };
    current[..index] == target[..index]
        && target[index] == canonical
        && !target[index + 1].is_empty()
        && target[index + 1].bytes().all(|byte| byte.is_ascii_digit())
        && current[index + 2..] == target[index + 2..]
}

pub(super) async fn get_json<T: DeserializeOwned>(
    source: &Source,
    path: &str,
    token: Option<String>,
) -> Result<T, (u16, String)> {
    Ok(Http::draft(source, token).await?.get(path).await?.data)
}

#[cfg(test)]
mod tests;
