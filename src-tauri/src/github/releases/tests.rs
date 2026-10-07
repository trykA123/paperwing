use super::*;
use crate::github::http::{Error, Response};
use crate::github::pulls::client::Transport;
use crate::github::pulls::repository::{parse_remote, Repository};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Exchange {
    method: String,
    path: String,
    status: u16,
    #[serde(default)]
    headers: HashMap<String, String>,
    body: Value,
}

struct Recorded {
    exchange: RefCell<Option<Exchange>>,
    bodies: RefCell<Vec<Value>>,
}

impl Recorded {
    fn new(fixture: &str) -> Self {
        Self {
            exchange: RefCell::new(Some(serde_json::from_str(fixture).unwrap())),
            bodies: RefCell::default(),
        }
    }
}

impl Transport for Recorded {
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Response, Error> {
        let exchange = self
            .exchange
            .borrow_mut()
            .take()
            .expect("Unexpected HTTP request");
        assert_eq!(method.as_str(), exchange.method);
        assert_eq!(path, exchange.path);
        self.bodies.borrow_mut().push(body.unwrap());
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
            body: serde_json::to_vec(&exchange.body).unwrap(),
            next: false,
        })
    }
}

fn enterprise() -> Repository {
    parse_remote(
        "ssh://git@gitint.company.com/admin/skein-fixture-api.git",
        "gitint.company.com",
    )
    .unwrap()
}

#[tokio::test]
async fn enterprise_release_uses_the_remote_host_and_registered_source() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/github");
    let settings = serde_json::from_value(json!({
        "sources": [{"id":"enterprise-source","kind":"ghe","host":"gitint.company.com","name":"Enterprise"}],
        "workspace": {"root":format!("{}/src", env!("CARGO_MANIFEST_DIR")),"layout":"flat","sets":[{
            "id":"fixture-set","name":"Fixture","items":[{
                "id":"fixture-item","repoId":"enterprise-source:admin/skein-fixture-api","name":"github",
                "url":"https://gitint.company.com/admin/skein-fixture-api.git","org":"admin","ref":{"type":"branch","name":"main"}
            }]
        }]}
    })).unwrap();
    let repo = enterprise();
    let source = source::for_path(&settings, &path, &repo).unwrap();
    assert_eq!(source.id, "enterprise-source");
    assert_eq!(
        crate::github::api_base(&source.host).unwrap(),
        "https://gitint.company.com/api/v3"
    );
    let recorded = Recorded::new(include_str!("fixtures/enterprise.json"));
    let release = client::create(&recorded, &repo, "v2.4.0", "Release notes", true)
        .await
        .unwrap();
    assert_eq!(
        release.url,
        "https://gitint.company.com/admin/skein-fixture-api/releases/33"
    );
    assert!(release.draft);
    assert_eq!(
        recorded.bodies.borrow()[0],
        json!({"tag_name":"v2.4.0","body":"Release notes","draft":true})
    );
    assert!(recorded.exchange.borrow().is_none());
}

#[tokio::test]
async fn release_rate_limit_serializes_the_reset_time_and_message() {
    let recorded = Recorded::new(include_str!("fixtures/rate-limited.json"));
    let error: PullsError = client::create(&recorded, &enterprise(), "v1", "", true)
        .await
        .unwrap_err()
        .into();
    let value = serde_json::to_value(error).unwrap();
    assert_eq!(value["kind"], "rateLimited");
    assert_eq!(
        value["resetAt"],
        chrono::DateTime::from_timestamp(1791331200, 0)
            .unwrap()
            .to_rfc3339()
    );
    assert!(value["message"].as_str().unwrap().contains("retry after"));
}

#[tokio::test]
async fn release_permission_denial_is_a_message_without_a_rate_limit() {
    let recorded = Recorded::new(include_str!("fixtures/permission.json"));
    let error: PullsError = client::create(&recorded, &enterprise(), "v1", "", true)
        .await
        .unwrap_err()
        .into();
    let value = serde_json::to_value(error).unwrap();
    assert_eq!(value["kind"], "message");
    assert!(value["message"].as_str().unwrap().contains("permissions"));
    assert!(value.get("resetAt").is_none());
}

struct Fixture {
    root: PathBuf,
    work: PathBuf,
    remote: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn git(path: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn fixture() -> Fixture {
    let root = PathBuf::from(crate::env_names::var_os("SKEIN_TEST_TMP").unwrap()).join(format!(
        "releases-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let work = root.join("work");
    let remote = root.join("remote.git");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&remote).unwrap();
    git(&remote, &["init", "--quiet", "--bare"]);
    git(&work, &["init", "--quiet"]);
    for (key, value) in [
        ("user.name", "admin"),
        ("user.email", "admin@example.invalid"),
        ("commit.gpgSign", "false"),
        ("tag.gpgSign", "false"),
        ("core.hooksPath", "."),
    ] {
        git(&work, &["config", key, value]);
    }
    git(
        &work,
        &["commit", "--quiet", "--allow-empty", "-m", "fixture"],
    );
    git(
        &work,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&work, &["tag", "-a", "-m", "Release notes", "v1"]);
    Fixture { root, work, remote }
}

#[tokio::test]
async fn release_requires_the_annotated_tag_on_the_remote_at_the_same_object() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    let path = fixture.work.to_str().unwrap();
    let url = fixture.remote.to_str().unwrap();
    let object = repository::annotated_object(path, "v1").await.unwrap();
    let error = repository::require_published(path, url, "v1", &object)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not on the remote"));
    git(
        &fixture.work,
        &["push", "--quiet", "origin", "refs/tags/v1"],
    );
    repository::require_published(path, url, "v1", &object)
        .await
        .unwrap();
    git(
        &fixture.remote,
        &[
            "update-ref",
            "refs/tags/v1",
            &git(&fixture.work, &["rev-parse", "HEAD"]),
        ],
    );
    assert!(repository::require_published(path, url, "v1", &object)
        .await
        .is_err());
}

#[tokio::test]
async fn release_refuses_lightweight_missing_and_invalid_tags() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    let path = fixture.work.to_str().unwrap();
    git(&fixture.work, &["tag", "light"]);
    for tag in ["light", "missing", "--delete", "HEAD"] {
        assert!(
            repository::annotated_object(path, tag).await.is_err(),
            "{tag}"
        );
    }
}

#[tokio::test]
async fn release_resolves_the_push_remote_host_instead_of_the_fetch_host() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    git(
        &fixture.work,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            "https://gitint.company.com/admin/skein-fixture-api.git",
        ],
    );
    let (repo, url) = repository::resolve(fixture.work.to_str().unwrap(), "origin")
        .await
        .unwrap();
    assert_eq!(repo, enterprise());
    assert!(url.starts_with("https://gitint.company.com/"));
}

#[test]
fn release_notes_are_bounded_and_reject_nulls() {
    assert!(client::validate_notes("").is_ok());
    assert!(client::validate_notes(&"a".repeat(64 * 1024 + 1)).is_err());
    assert!(client::validate_notes("a\0b").is_err());
}

#[tokio::test]
async fn release_rejects_response_links_outside_the_repository_host() {
    for url in [
        "javascript:alert(1)",
        "https://other.example/admin/repo/releases/33",
        "https://user:password@gitint.company.com/releases/33",
    ] {
        let recorded = Recorded::new(&json!({"method":"POST","path":"/repos/admin/skein-fixture-api/releases","status":201,"body":{"id":33,"html_url":url,"draft":true}}).to_string());
        let error = client::create(&recorded, &enterprise(), "v1", "", true)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "Unexpected release URL from GitHub");
    }
}

#[tokio::test]
async fn release_can_be_explicitly_published_instead_of_saved_as_draft() {
    let recorded = Recorded::new(&json!({"method":"POST","path":"/repos/admin/skein-fixture-api/releases","status":201,"body":{"id":33,"html_url":"https://gitint.company.com/admin/skein-fixture-api/releases/33","draft":false}}).to_string());
    let release = client::create(&recorded, &enterprise(), "v1", "", false)
        .await
        .unwrap();
    assert!(!release.draft);
    assert_eq!(recorded.bodies.borrow()[0]["draft"], false);
}
