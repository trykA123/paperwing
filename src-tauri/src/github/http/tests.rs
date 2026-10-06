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
            base: "http://127.0.0.1:41000".into(),
            host: &source.host,
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
                base: "http://127.0.0.1:41000".into(),
                host: &source.host,
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
    for (current, target) in [
        (
            "https://api.github.com/orgs/acme/repos?type=all&per_page=100&page=1",
            "https://api.github.com/organizations/123456/repos?type=all&per_page=100&page=2",
        ),
        (
            "https://enterprise.invalid/api/v3/orgs/acme/repos?page=2",
            "https://enterprise.invalid/api/v3/organizations/42/repos?page=3",
        ),
        (
            "https://api.github.com/users/admin/repos?type=owner&page=1",
            "https://api.github.com/user/987/repos?type=owner&page=2",
        ),
    ] {
        assert!(next_page(current, Some(&format!("<{target}>; rel=\"next\""))).unwrap());
    }
    for target in [
        "https://other.invalid/api/v3/user/repos?page=2",
        "https://enterprise.invalid/api/v3/organizations/42/repos?page=2",
        "https://enterprise.invalid/api/v3/organizations/x1/repos?page=2",
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
        base: "http://127.0.0.1:41000".into(),
        host: &source.host,
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

async fn fixture_http<'a>(source: &'a Source, host: &'a str, token: &str) -> Http<'a> {
    Http::with_token(
        Connection {
            source,
            base: api_base(host).unwrap(),
            host,
            expected: 0,
        },
        async { Ok(Some(token.into())) },
        || 0,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn shared_get_keeps_enterprise_messages_and_skips_error_bodies() {
    let source: Source = serde_json::from_value(serde_json::json!({"id":"legacy-http-fixture","name":"admin","kind":"ghe","host":"gitext.company.com"})).unwrap();
    let http = fixture_http(&source, &source.host, "synthetic-token").await;
    assert_eq!(
        http.connection_error().to_string(),
        "Cannot reach gitext.company.com"
    );
    for (status, message) in [
        (401, "Token is missing or invalid"),
        (
            403,
            "Access denied or rate limited; check token repository permissions",
        ),
        (404, "Not found"),
        (500, "HTTP 500"),
    ] {
        let result = response::shared_page::<serde_json::Value>(&source.host, status, async {
            panic!("non-success response body must not be read")
        })
        .await;
        assert_eq!(result.err().unwrap(), (status, message.into()));
    }
    let response = Response {
        status: 200,
        headers: Default::default(),
        body: b"invalid".to_vec(),
        next: false,
    };
    let result =
        response::shared_page::<serde_json::Value>(&source.host, 200, async { Ok(response) }).await;
    assert_eq!(
        result.err().unwrap(),
        (0, "Unexpected response from gitext.company.com".into())
    );
    let result = response::shared_page::<serde_json::Value>(&source.host, 200, async {
        Err(Error::Message("Cannot read GitHub response".into()))
    })
    .await;
    assert_eq!(
        result.err().unwrap(),
        (0, "Cannot read GitHub response".into())
    );
}

#[tokio::test]
async fn configured_enterprise_and_public_tokens_stay_on_their_own_hosts() {
    let sources: Vec<Source> =
        serde_json::from_str(include_str!("../pulls/fixtures/enterprise_sources.json")).unwrap();
    for source in &sources {
        for destination in &sources {
            let token = format!("synthetic-{}-token", source.id);
            let http = fixture_http(source, &destination.host, &token).await;
            let request = http
                .build_request(Method::GET, "/repos/admin/repo", None)
                .unwrap();
            if source.host == destination.host {
                assert_eq!(
                    request.headers()["authorization"],
                    format!("Bearer {token}")
                );
                assert!(http.authenticated());
            } else {
                assert!(request.headers().get("authorization").is_none());
                assert!(!http.authenticated());
            }
            let expected = if destination.kind == "ghe" {
                format!("https://{}/api/v3/repos/admin/repo", destination.host)
            } else {
                "https://api.github.com/repos/admin/repo".into()
            };
            assert_eq!(request.url().as_str(), expected);
        }
    }
}

#[tokio::test]
async fn manual_tokens_require_every_url_to_belong_to_the_api_host() {
    let mut source: Source = serde_json::from_value(serde_json::json!({"id":"manual-http-fixture","name":"admin","kind":"manual","host":"gitext.company.com"})).unwrap();
    for (urls, authenticated) in [
        (
            vec![
                "https://gitext.company.com/admin/repo.git",
                "ssh://git@gitext.company.com:2222/admin/other.git",
                "git@gitext.company.com:admin/third.git",
            ],
            true,
        ),
        (
            vec![
                "https://gitext.company.com/admin/repo.git",
                "https://gitint.company.com/admin/repo.git",
            ],
            false,
        ),
        (
            vec!["https://gitext.company.com/admin/repo.git", "invalid"],
            false,
        ),
        (vec!["https://github.com/admin/repo.git"], false),
        (vec![], false),
    ] {
        source.urls = urls.into_iter().map(String::from).collect();
        let http = fixture_http(&source, "gitext.company.com", "synthetic-manual-token").await;
        let request = http.build_request(Method::GET, "/user", None).unwrap();
        assert_eq!(
            request.headers().contains_key("authorization"),
            authenticated
        );
    }
    source.host.clear();
    source.urls = vec!["https://gitext.company.com/admin/repo.git".into()];
    assert!(
        fixture_http(&source, "gitext.company.com", "synthetic-manual-token")
            .await
            .authenticated()
    );
}

#[tokio::test]
async fn unrelated_sources_do_not_acquire_a_token() {
    let source: Source = serde_json::from_value(serde_json::json!({"id":"unrelated-http-fixture","name":"admin","kind":"ghe","host":"gitint.company.com"})).unwrap();
    let http = Http::with_token(
        Connection {
            source: &source,
            base: api_base("gitext.company.com").unwrap(),
            host: "gitext.company.com",
            expected: 0,
        },
        async { panic!("another hosts credentials must not be read") },
        || 0,
    )
    .await
    .unwrap();
    assert!(!http.authenticated());
}

#[tokio::test]
async fn enterprise_retries_a_version_rejection_once_without_changing_token_or_body() {
    use std::cell::RefCell;
    let source: Source = serde_json::from_value(serde_json::json!({"id":"version-http-fixture","name":"admin","kind":"ghe","host":"gitext.company.com"})).unwrap();
    for statuses in [[400, 200], [400, 400]] {
        let http = fixture_http(&source, &source.host, "synthetic-version-token").await;
        let request = http
            .build_request(
                Method::POST,
                "/repos/admin/repo/pulls",
                Some(serde_json::json!({"head":"feature"})),
            )
            .unwrap();
        let requests = RefCell::new(Vec::new());
        let status = http
            .dispatch_request(request, |request| {
                let index = requests.borrow().len();
                requests.borrow_mut().push(request);
                async move { Ok((statuses[index], statuses[index])) }
            })
            .await
            .unwrap();
        assert_eq!(status, statuses[1]);
        let requests = requests.borrow();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].headers().contains_key("x-github-api-version"));
        assert!(!requests[1].headers().contains_key("x-github-api-version"));
        for request in requests.iter() {
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-version-token"
            );
            assert_eq!(
                request.body().unwrap().as_bytes().unwrap(),
                br#"{"head":"feature"}"#
            );
        }
        assert!(!http
            .build_request(Method::GET, "/user", None)
            .unwrap()
            .headers()
            .contains_key("x-github-api-version"));
    }
}

#[tokio::test]
async fn public_github_and_ordinary_enterprise_errors_do_not_retry() {
    use std::cell::Cell;
    for (host, status) in [
        ("github.com", 400),
        ("GitHub.COM", 400),
        ("gitext.company.com", 401),
        ("gitext.company.com", 403),
        ("gitext.company.com", 429),
    ] {
        let source: Source = serde_json::from_value(serde_json::json!({"id":"no-retry-http-fixture","name":"admin","kind":"ghe","host":host})).unwrap();
        let http = fixture_http(&source, host, "synthetic-token").await;
        let calls = Cell::new(0);
        let request = http.build_request(Method::GET, "/user", None).unwrap();
        if host.eq_ignore_ascii_case("github.com") {
            assert_eq!(request.url().as_str(), "https://api.github.com/user");
        }
        let result = http
            .dispatch_request(request, |_| {
                calls.set(calls.get() + 1);
                async { Ok((status, status)) }
            })
            .await
            .unwrap();
        assert_eq!(result, status);
        assert_eq!(calls.get(), 1);
    }
}

#[tokio::test]
async fn typed_draft_token_works_on_a_new_unsaved_host() {
    let source: Source = serde_json::from_value(serde_json::json!({"id":"unsaved-draft-fixture","name":"admin","kind":"ghe","host":"new.invalid"})).unwrap();
    let http = Http::draft(&source, Some("synthetic-typed-token".into())).await.unwrap();
    let request = http.build_request(Method::GET, "/user", None).unwrap();
    assert_eq!(request.headers()["authorization"], "Bearer synthetic-typed-token");
    assert_eq!(request.url().host_str(), Some("new.invalid"));
    assert!(Http::draft(&source, None).await.is_err());
}

#[tokio::test]
async fn bound_token_mismatch_stops_the_counting_transport() {
    use std::cell::Cell;
    let source: Source = serde_json::from_value(serde_json::json!({"id":"bound-draft-fixture","name":"admin","kind":"ghe","host":"new.invalid"})).unwrap();
    let sends = Cell::new(0);
    let connection = Http::with_token(
        Connection { source: &source, base: api_base(&source.host).unwrap(), host: &source.host, expected: 0 },
        async { crate::credentials::token_fixture(r#"skein-token-v1:{"host":"saved.invalid","token":"synthetic-stored-token"}"#, &source.host) },
        || 0,
    ).await;
    if let Ok(http) = connection {
        let request = http.build_request(Method::GET, "/user", None).unwrap();
        http.dispatch_request(request, |_| async { sends.set(sends.get() + 1); Ok((200, ())) }).await.unwrap();
    }
    assert_eq!(sends.get(), 0);
}
