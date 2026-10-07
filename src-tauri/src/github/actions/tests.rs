use super::{
    client::Client,
    commands::require_confirmation,
    transport::{Request, Transport},
};
use crate::{
    github::{
        http::{Error, Response},
        pulls::repository::Repository,
    },
    kernel::{
        capabilities::{CiProvider, ProviderFuture},
        ci::*,
    },
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};

#[derive(Deserialize)]
struct Exchange {
    kind: String,
    path: String,
    status: u16,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Value,
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    next: bool,
}

struct Recorded(Mutex<VecDeque<Exchange>>);

impl Recorded {
    fn new(fixture: &str) -> Self {
        Self(Mutex::new(serde_json::from_str(fixture).unwrap()))
    }
    fn complete(&self) {
        assert!(self.0.lock().unwrap().is_empty());
    }
}

impl Transport for Recorded {
    fn send<'a>(&'a self, request: Request<'a>) -> ProviderFuture<'a, Result<Response, Error>> {
        Box::pin(async move {
            let exchange = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("recorded CI request");
            let (kind, path, etag, body) = match request {
                Request::Metadata { path, etag } => ("metadata", path, etag, None),
                Request::Write { path, body } => ("write", path, None, body),
                Request::Raw { path } => ("raw", path, None, None),
                Request::Download { path } => ("download", path, None, None),
            };
            assert_eq!(kind, exchange.kind);
            assert_eq!(path, exchange.path);
            assert_eq!(etag, exchange.etag.as_deref());
            if kind == "write" {
                assert_eq!(body.unwrap_or(Value::Null), exchange.body);
            }
            let mut headers = HeaderMap::new();
            for (name, value) in exchange.headers {
                headers.insert(
                    HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    HeaderValue::from_str(&value).unwrap(),
                );
            }
            Ok(Response {
                status: exchange.status,
                headers,
                body: exchange
                    .raw
                    .map(String::into_bytes)
                    .unwrap_or_else(|| serde_json::to_vec(&exchange.body).unwrap()),
                next: exchange.next,
            })
        })
    }
}

fn enterprise() -> Repository {
    Repository {
        host: "enterprise.invalid".into(),
        owner: "admin".into(),
        name: "repo".into(),
    }
}

#[tokio::test]
async fn neutral_runs_paginate_and_revalidate_on_an_enterprise_host() {
    let transport = Recorded::new(include_str!("fixtures/runs.json"));
    let repo = enterprise();
    assert_eq!(
        crate::github::api_base(&repo.host).unwrap(),
        "https://enterprise.invalid/api/v3"
    );
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    let first = CiProvider::runs(
        &provider,
        &CiQuery {
            branch: Some("main".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    match first {
        CiPage::Updated {
            items,
            etag,
            next_page,
        } => {
            assert_eq!(next_page, Some(2));
            assert_eq!(etag.as_deref(), Some("\"runs-1\""));
            assert_eq!(items[0].provider, "github-actions");
            assert_eq!(items[0].host, repo.host);
            assert_eq!(items[0].status, CiStatus::Failed);
            assert_eq!(items[0].duration_seconds, Some(90));
        }
        _ => panic!("updated first page"),
    }
    let second = CiProvider::runs(
        &provider,
        &CiQuery {
            branch: Some("main".into()),
            page: Some(2),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        second,
        CiPage::Updated {
            next_page: None,
            ..
        }
    ));
    let query = CiQuery {
        branch: Some("main".into()),
        etag: Some("\"runs-1\"".into()),
        ..Default::default()
    };
    assert!(matches!(
        CiProvider::runs(&provider, &query).await.unwrap(),
        CiPage::NotModified { .. }
    ));
    transport.complete();
}

#[tokio::test]
async fn rate_limits_expose_reset_time_and_permission_errors_name_actions_access() {
    let repo = enterprise();
    let transport = Recorded::new(include_str!("fixtures/errors.json"));
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    let error = CiProvider::runs(&provider, &CiQuery::default())
        .await
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({"kind":"rateLimited","resetAt":"2026-10-07T16:00:00+00:00","message":"CI rate limit reached; retry after 2026-10-07 16:00:00 UTC"})
    );
    let read = CiProvider::runs(&provider, &CiQuery::default())
        .await
        .unwrap_err();
    assert!(read.to_string().contains("Actions read permission"));
    let write = CiProvider::rerun(&provider, "31", true).await.unwrap_err();
    assert!(write.to_string().contains("Actions write permission"));
    assert!(!write.to_string().contains("malicious-secret"));
    transport.complete();
}

#[tokio::test]
async fn jobs_steps_logs_and_artifacts_use_neutral_records_without_persistence() {
    let repo = enterprise();
    let transport = Recorded::new(include_str!("fixtures/jobs.json"));
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    match CiProvider::jobs(&provider, "31", &CiQuery::default())
        .await
        .unwrap()
    {
        CiPage::Updated { items, .. } => {
            assert_eq!(items[0].status, CiStatus::Failed);
            assert_eq!(items[0].steps[0].status, CiStatus::Failed);
            assert_eq!(items[0].steps[0].provider, "github-actions");
            assert_eq!(items[0].steps[0].host, repo.host);
            assert_eq!(items[0].duration_seconds, Some(60));
        }
        _ => panic!("updated jobs"),
    }
    let log = CiProvider::log(&provider, "32").await.unwrap();
    assert!(log.text.contains("##[group]"));
    assert_eq!(log.host, repo.host);
    assert!(
        matches!(CiProvider::artifacts(&provider, "31", &CiQuery::default()).await.unwrap(), CiPage::Updated { items, .. } if items[0].size_bytes == 4)
    );
    let artifact = CiProvider::download_artifact(&provider, "33")
        .await
        .unwrap();
    assert_eq!(artifact.bytes, b"PKfixture");
    assert_eq!(artifact.filename, "artifact-33.zip");
    assert_eq!(
        CiProvider::download_logs(&provider, "31")
            .await
            .unwrap()
            .bytes,
        b"PKlogs"
    );
    transport.complete();
}

#[tokio::test]
async fn incomplete_jobs_never_request_logs() {
    let transport = Recorded::new(
        r#"[{"kind":"metadata","path":"/repos/admin/repo/actions/jobs/32","status":200,"body":{"status":"in_progress"}}]"#,
    );
    let repo = enterprise();
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    let error = CiProvider::log(&provider, "32").await.unwrap_err();
    assert!(error.to_string().contains("after the job finishes"));
    transport.complete();
}

#[tokio::test]
async fn explicit_actions_rerun_cancel_and_dispatch_after_loading_the_chosen_ref() {
    let transport = Recorded::new(include_str!("fixtures/actions.json"));
    let repo = enterprise();
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    assert!(
        CiProvider::rerun(&provider, "31", false)
            .await
            .unwrap()
            .accepted
    );
    assert!(
        CiProvider::rerun(&provider, "31", true)
            .await
            .unwrap()
            .accepted
    );
    let cancelled = CiProvider::cancel_run(&provider, "31").await.unwrap();
    assert_eq!(cancelled.host, repo.host);
    let request = CiDispatch {
        pipeline_id: "7".into(),
        reference: "release/1".into(),
        inputs: BTreeMap::from([("dry".into(), json!(false))]),
    };
    let receipt = CiProvider::start(&provider, request).await.unwrap();
    assert!(receipt.accepted);
    assert_eq!(receipt.run_id, None);
    transport.complete();
}

#[tokio::test]
async fn invalid_ids_and_pages_never_send_requests() {
    let transport = Recorded::new("[]");
    let repo = enterprise();
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    for id in ["", "0", "-1", "31/cancel", "31?secret=value"] {
        assert!(CiProvider::rerun(&provider, id, false).await.is_err());
        assert!(CiProvider::log(&provider, id).await.is_err());
    }
    assert!(CiProvider::runs(
        &provider,
        &CiQuery {
            page: Some(0),
            ..Default::default()
        }
    )
    .await
    .is_err());
    transport.complete();
}

#[test]
fn actions_require_confirmation_before_provider_resolution() {
    assert!(require_confirmation(false).is_err());
    assert!(require_confirmation(true).is_ok());
}

#[tokio::test]
async fn configured_enterprise_repository_reads_need_no_local_checkout() {
    let settings = serde_json::from_value(json!({
        "sources":[{"id":"remote-ci-source","name":"admin","kind":"ghe","host":"enterprise.invalid"}],
        "workspace":{"sets":[{"id":"ci-set","items":[{"repoId":"remote-ci-source:admin/repo","url":"ssh://git@enterprise.invalid/admin/repo.git"}]}]}
    })).unwrap();
    let (source, repo) = crate::github::pulls::source::for_remote(
        &settings,
        "remote-ci-source",
        "https://enterprise.invalid/admin/repo.git",
    )
    .unwrap();
    assert_eq!(source.host, "enterprise.invalid");
    assert_eq!(repo.host, source.host);
    let transport = Recorded::new(
        r#"[{"kind":"metadata","path":"/repos/admin/repo/actions/runs?per_page=100&page=1","status":200,"body":{"workflow_runs":[]}}]"#,
    );
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    assert!(matches!(
        CiProvider::runs(&provider, &CiQuery::default())
            .await
            .unwrap(),
        CiPage::Updated { .. }
    ));
    for url in [
        "https://other.invalid/admin/repo.git",
        "https://enterprise.invalid/admin/unregistered.git",
    ] {
        assert!(
            crate::github::pulls::source::for_remote(&settings, "remote-ci-source", url).is_err()
        );
    }
    transport.complete();
}

#[tokio::test]
async fn dispatch_invalid_inputs_load_the_selected_ref_without_posting() {
    let transport = Recorded::new(
        r#"[
        {"kind":"metadata","path":"/repos/admin/repo/actions/workflows/7","status":200,"body":{"path":".github/workflows/build.yml"}},
        {"kind":"raw","path":"/repos/admin/repo/contents/.github/workflows/build.yml?ref=release%2F1","status":200,"raw":"on:\n  workflow_dispatch:\n    inputs:\n      dry:\n        type: boolean\n"}
    ]"#,
    );
    let repo = enterprise();
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    let request = CiDispatch {
        pipeline_id: "7".into(),
        reference: "release/1".into(),
        inputs: BTreeMap::from([("dry".into(), json!("not-a-boolean"))]),
    };
    assert!(CiProvider::start(&provider, request)
        .await
        .unwrap_err()
        .to_string()
        .contains("Invalid CI input"));
    transport.complete();
}

#[tokio::test]
async fn dispatch_configuration_denial_names_contents_read_permission() {
    let transport = Recorded::new(
        r#"[
        {"kind":"metadata","path":"/repos/admin/repo/actions/workflows/7","status":200,"body":{"path":".github/workflows/build.yml"}},
        {"kind":"raw","path":"/repos/admin/repo/contents/.github/workflows/build.yml?ref=main","status":403,"body":{"message":"denied"}}
    ]"#,
    );
    let repo = enterprise();
    let provider = Client {
        transport: &transport,
        repo: &repo,
    };
    let request = CiDispatch {
        pipeline_id: "7".into(),
        reference: "main".into(),
        inputs: BTreeMap::new(),
    };
    assert!(CiProvider::start(&provider, request)
        .await
        .unwrap_err()
        .to_string()
        .contains("Contents read permission"));
    transport.complete();
}
