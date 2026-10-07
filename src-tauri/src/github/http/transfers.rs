use super::{Error, Http, Response};
use crate::kernel::capabilities::ProviderFuture;
use reqwest::{
    header::{ACCEPT, IF_NONE_MATCH, LOCATION},
    Method, Url,
};

const DOWNLOAD_LIMIT: usize = 64 * 1024 * 1024;

impl Http<'_> {
    pub(crate) async fn conditional(
        &self,
        path: &str,
        etag: Option<&str>,
    ) -> Result<Response, Error> {
        let mut request = self.build_request(Method::GET, path, None)?;
        if let Some(etag) = etag {
            let value = etag
                .parse()
                .map_err(|_| Error::Message("Invalid CI response validator".into()))?;
            request.headers_mut().insert(IF_NONE_MATCH, value);
        }
        let response = self.dispatch_built(request).await?;
        let response = self
            .read_response(&format!("{}{path}", self.base), response)
            .await?;
        self.check_revision()?;
        Ok(response)
    }

    pub(crate) async fn raw(&self, path: &str) -> Result<Response, Error> {
        let mut request = self.build_request(Method::GET, path, None)?;
        request.headers_mut().insert(
            ACCEPT,
            "application/vnd.github.raw+json"
                .parse()
                .map_err(|_| Error::Message("Cannot request workflow content".into()))?,
        );
        let response = self.dispatch_built(request).await?;
        let response = self
            .read_response(&format!("{}{path}", self.base), response)
            .await?;
        self.check_revision()?;
        Ok(response)
    }

    pub(crate) async fn download(&self, path: &str) -> Result<Response, Error> {
        let lease =
            crate::providers::acquire(self.source, &self.api_host, None).map_err(Error::Message)?;
        lease
            .run(self.fetch_download(path))
            .await
            .map_err(|error| Error::Message(error.to_string()))?
    }

    async fn fetch_download(&self, path: &str) -> Result<Response, Error> {
        self.download_with(path, |request| {
            Box::pin(async {
                let url = request.url().to_string();
                let response = self
                    .client
                    .execute(request)
                    .await
                    .map_err(|_| Error::Message("Cannot download CI content".into()))?;
                self.read_bounded(&url, response, DOWNLOAD_LIMIT).await
            })
        })
        .await
    }

    async fn download_with<'a>(
        &'a self,
        path: &str,
        send: impl Fn(reqwest::Request) -> ProviderFuture<'a, Result<Response, Error>>,
    ) -> Result<Response, Error> {
        let mut url = Url::parse(&format!("{}{path}", self.base))
            .map_err(|_| Error::Message("Invalid CI download path".into()))?;
        let request = self.build_request(Method::GET, path, None)?;
        let mut response = self
            .dispatch_request(request, |request| async {
                let response = send(request).await?;
                Ok((response.status, response))
            })
            .await?;
        for attempt in 0..=5 {
            if response.body.len() > DOWNLOAD_LIMIT {
                return Err(Error::Message(
                    "GitHub response exceeds download limit".into(),
                ));
            }
            if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
                self.check_revision()?;
                return Ok(response);
            }
            if attempt == 5 {
                return Err(Error::Message("Too many CI download redirects".into()));
            }
            let location = response
                .headers
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| Error::Message("CI download redirect has no destination".into()))?;
            url = redirect_target(&url, location)?;
            self.check_revision()?;
            response = send(self.download_request(&url)?).await?;
        }
        Err(Error::Message("Too many CI download redirects".into()))
    }

    fn download_request(&self, url: &Url) -> Result<reqwest::Request, Error> {
        let mut request = self.client.get(url.clone());
        let base =
            Url::parse(&self.base).map_err(|_| Error::Message("Invalid CI API host".into()))?;
        if url.origin() == base.origin() {
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
        }
        request
            .build()
            .map_err(|_| Error::Message("Cannot download CI content".into()))
    }
}

fn redirect_target(current: &Url, location: &str) -> Result<Url, Error> {
    let invalid = || Error::Message("Unsafe CI download redirect".into());
    let target = current.join(location).map_err(|_| invalid())?;
    if target.scheme() != "https"
        || target.host_str().is_none()
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::http::Connection;
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    fn response(status: u16, location: Option<&str>, body: Vec<u8>) -> Response {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(location) = location {
            headers.insert(LOCATION, location.parse().unwrap());
        }
        Response {
            status,
            headers,
            body,
            next: false,
        }
    }

    async fn connect(source: &crate::settings::Source) -> Http<'_> {
        Http::with_token(
            Connection {
                source,
                base: "https://enterprise.invalid/api/v3".into(),
                host: &source.host,
                expected: 0,
            },
            async { Ok(Some("synthetic-ci-token".into())) },
            || 0,
        )
        .await
        .unwrap()
    }

    fn source(id: &str) -> crate::settings::Source {
        serde_json::from_value(
            serde_json::json!({"id":id,"name":"admin","kind":"ghe","host":"enterprise.invalid"}),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn recorded_download_loop_follows_redirects_without_external_credentials() {
        let source = source("ci-loop-source");
        let http = connect(&source).await;
        let responses = Mutex::new(VecDeque::from([
            response(
                302,
                Some("https://downloads.invalid/signed?fixture=1"),
                Vec::new(),
            ),
            response(307, Some("https://storage.invalid/log"), Vec::new()),
            response(200, None, b"completed job log".to_vec()),
        ]));
        let requests = Mutex::new(Vec::new());
        let result = http
            .download_with("/repos/admin/repo/actions/jobs/32/logs", |request| {
                requests.lock().unwrap().push((
                    request.url().host_str().unwrap().to_string(),
                    request.headers().contains_key("authorization"),
                ));
                let response = responses.lock().unwrap().pop_front().unwrap();
                Box::pin(async move { Ok(response) })
            })
            .await
            .unwrap();
        assert_eq!(result.body, b"completed job log");
        assert_eq!(
            *requests.lock().unwrap(),
            [
                ("enterprise.invalid".into(), true),
                ("downloads.invalid".into(), false),
                ("storage.invalid".into(), false)
            ]
        );
        assert!(responses.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn recorded_download_loop_rejects_invalid_missing_and_excessive_redirects() {
        let source = source("ci-loop-errors-source");
        let http = connect(&source).await;
        for location in [
            None,
            Some("http://downloads.invalid/log"),
            Some("https://user:secret@downloads.invalid/log"),
        ] {
            let calls = AtomicUsize::new(0);
            let error = http
                .download_with("/logs", |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move { Ok(response(302, location, Vec::new())) })
                })
                .await
                .err()
                .unwrap();
            assert!(error.to_string().contains("redirect"));
            assert!(!error.to_string().contains("secret"));
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
        let calls = AtomicUsize::new(0);
        let result = http
            .download_with("/logs", |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Ok(response(
                        302,
                        Some("https://downloads.invalid/loop"),
                        Vec::new(),
                    ))
                })
            })
            .await;
        assert!(result.err().unwrap().to_string().contains("Too many"));
        assert_eq!(calls.load(Ordering::SeqCst), 6);
    }

    #[tokio::test]
    async fn recorded_download_loop_enforces_size_limits_and_returns_http_failures() {
        let source = source("ci-loop-limit-source");
        let http = connect(&source).await;
        let result = http
            .download_with("/logs", |_| {
                Box::pin(async { Ok(response(200, None, vec![0; DOWNLOAD_LIMIT + 1])) })
            })
            .await;
        assert!(result.err().unwrap().to_string().contains("download limit"));
        let result = http
            .download_with("/logs", |_| {
                Box::pin(async { Ok(response(410, None, Vec::new())) })
            })
            .await
            .unwrap();
        assert!(matches!(
            result.checked(),
            Err(Error::Http { status: 410, .. })
        ));
    }

    #[test]
    fn rejects_download_redirects_with_unsafe_schemes_credentials_or_fragments() {
        let current = Url::parse("https://enterprise.invalid/api/v3/logs").unwrap();
        assert_eq!(
            redirect_target(&current, "/download").unwrap().host_str(),
            Some("enterprise.invalid")
        );
        for target in [
            "http://external.invalid/log",
            "file:///log",
            "https://user:secret@external.invalid/log",
            "https://external.invalid/log#part",
        ] {
            assert!(redirect_target(&current, target).is_err());
        }
    }

    #[tokio::test]
    async fn external_download_targets_never_receive_credentials() {
        let source = serde_json::from_value(serde_json::json!({"id":"ci-redirect-source","name":"admin","kind":"ghe","host":"enterprise.invalid"})).unwrap();
        let http = Http::with_token(
            Connection {
                source: &source,
                base: "https://enterprise.invalid/api/v3".into(),
                host: "enterprise.invalid",
                expected: 0,
            },
            async { Ok(Some("synthetic-ci-token".into())) },
            || 0,
        )
        .await
        .unwrap();
        let same = http
            .download_request(&Url::parse("https://enterprise.invalid/download").unwrap())
            .unwrap();
        assert!(same.headers().contains_key("authorization"));
        for target in [
            "https://external.invalid/log?signature=fixture",
            "https://enterprise.invalid:444/log",
        ] {
            let request = http.download_request(&Url::parse(target).unwrap()).unwrap();
            for header in [
                "authorization",
                "cookie",
                "proxy-authorization",
                "x-github-api-version",
            ] {
                assert!(!request.headers().contains_key(header));
            }
        }
    }
}
