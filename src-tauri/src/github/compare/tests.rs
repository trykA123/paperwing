use super::fixture::{page, source, Reply, Server};
use super::*;
use crate::github::http::fixture::Binding;

#[tokio::test]
async fn configured_public_and_enterprise_hosts_paginate_and_conservatively_truncate() {
    for host in ["github.com", "gitext.company.com"] {
        let source = source(host);
        let server = Server::new(|request, base| {
            let path = request.split_whitespace().nth(1).unwrap();
            if path.contains("/commits/heads%2F") { return Reply::json(serde_json::json!({"sha": if path.ends_with("main") {"a".repeat(40)} else {"b".repeat(40)}})); }
            let number: usize = path.rsplit('=').next().unwrap().parse().unwrap();
            let mut data = page(if number == 1 { 300 } else { 0 }, 100);
            for (index, commit) in data["commits"].as_array_mut().unwrap().iter_mut().enumerate() {
                commit["sha"] = format!("{:040x}", (number - 1) * 100 + index).into();
            }
            let mut response = Reply::json(data);
            response.headers = format!("Link: <{base}{}={}>; rel=\"next\"\r\n", path.rsplit_once('=').unwrap().0, number + 1);
            response
        }).await;
        let _binding = Binding::new(&source.id, &server.base);
        let request = Request::from_url(
            source.clone(),
            &format!("https://{host}/admin/repo.git"),
            ["heads/main".into(), "heads/topic".into()],
        )
        .unwrap();
        let http = request.connect().await.unwrap();
        let result = fetch(&http, &request).await.unwrap();
        assert_eq!(result.files.len(), 300);
        assert_eq!(result.commits.len(), 250);
        assert_eq!(
            result
                .commits
                .iter()
                .map(|commit| &commit.sha)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            250
        );
        assert_eq!(result.files[0].line_counts(), Some((1, 1)));
        assert!(result.truncated.files && result.truncated.commits);
        assert_eq!(server.count(), 5);
        assert!(server
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| !request.contains("page=4")));
    }
}

#[tokio::test]
async fn not_found_and_primary_and_secondary_limits_stop_without_retry() {
    for host in ["github.com", "gitint.company.com"] {
        for (status, headers, body, limited) in [
            (404, "", "missing", false),
            (
                403,
                "X-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: 1900000000\r\n",
                "limited",
                true,
            ),
            (429, "Retry-After: 120\r\n", "limited", true),
            (403, "", "{\"message\":\"secondary rate limit\"}", true),
            (200, "X-RateLimit-Remaining: 0\r\n", "{}", true),
        ] {
            let source = source(host);
            let server = Server::new(move |_, _| Reply {
                status,
                headers: headers.into(),
                body: body.as_bytes().to_vec(),
            })
            .await;
            let _binding = Binding::new(&source.id, &server.base);
            let request = Request::from_url(
                source.clone(),
                &format!("https://{host}/admin/repo.git"),
                ["a".repeat(40), "b".repeat(40)],
            )
            .unwrap();
            let http = request.connect().await.unwrap();
            let error = fetch(&http, &request).await.unwrap_err();
            assert_eq!(matches!(error, Error::RateLimited { .. }), limited);
            if !limited {
                assert!(matches!(error, Error::Http { status: 404, .. }));
            }
            assert_eq!(server.count(), 1);
        }
    }
}

#[tokio::test]
async fn small_comparison_maps_first_page_only_and_disabled_source_does_not_fetch() {
    let mut source = source("gitext.company.com");
    let server = Server::new(|_, _| {
        let mut data = page(1, 1);
        data["total_commits"] = 1.into();
        Reply::json(data)
    })
    .await;
    let _binding = Binding::new(&source.id, &server.base);
    let refs = ["a".repeat(40), "b".repeat(40)];
    let request = Request::from_url(
        source.clone(),
        "https://gitext.company.com/admin/repo.git",
        refs.clone(),
    )
    .unwrap();
    let http = request.connect().await.unwrap();
    let result = fetch(&http, &request).await.unwrap();
    assert!(!result.truncated.files && !result.truncated.commits);
    source.enabled = false;
    let request =
        Request::from_url(source, "https://gitext.company.com/admin/repo.git", refs).unwrap();
    assert!(request.connect().await.is_err());
    assert_eq!(server.count(), 1);
}
