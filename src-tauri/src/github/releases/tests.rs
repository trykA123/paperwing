use super::*;
use crate::github::http::{Error, Response};
use crate::github::pulls::repository::{parse_remote, Repository};
use crate::github::pulls::{client::Transport, source};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Deserialize)]
struct Exchange {
    method: String,
    path: String,
    status: u16,
    #[serde(default)]
    headers: HashMap<String, String>,
    body: Value,
}

#[derive(Clone)]
struct Recorded {
    exchange: Rc<RefCell<Option<Exchange>>>,
    bodies: Rc<RefCell<Vec<Value>>>,
    authenticated: bool,
}

impl Recorded {
    fn new(fixture: &str) -> Self {
        Self {
            exchange: Rc::new(RefCell::new(Some(serde_json::from_str(fixture).unwrap()))),
            bodies: Rc::default(),
            authenticated: true,
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

impl ReleaseTransport for Recorded {
    fn authenticated(&self) -> bool {
        self.authenticated
    }
}

fn release_request<'a>(tag: &'a str, notes: &'a str, draft: bool) -> client::CreateRelease<'a> {
    client::CreateRelease {
        tag,
        commit: "cccccccccccccccccccccccccccccccccccccccc",
        notes,
        draft,
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
    let release = client::create(
        &recorded,
        &repo,
        &release_request("v2.4.0", "Release notes", true),
    )
    .await
    .unwrap();
    assert_eq!(
        release.url,
        "https://gitint.company.com/admin/skein-fixture-api/releases/33"
    );
    assert!(release.draft);
    assert_eq!(
        recorded.bodies.borrow()[0],
        json!({"tag_name":"v2.4.0","target_commitish":"cccccccccccccccccccccccccccccccccccccccc","body":"Release notes","draft":true})
    );
    assert!(recorded.exchange.borrow().is_none());
}

#[tokio::test]
async fn release_rate_limit_serializes_the_reset_time_and_message() {
    let recorded = Recorded::new(include_str!("fixtures/rate-limited.json"));
    let error: PullsError =
        client::create(&recorded, &enterprise(), &release_request("v1", "", true))
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
    let error: PullsError =
        client::create(&recorded, &enterprise(), &release_request("v1", "", true))
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
    let root = crate::test_support::tmp_root().join(format!(
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
    let error = repository::require_published(path, url, "v1", &object.object)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not on the remote"));
    git(
        &fixture.work,
        &["push", "--quiet", "origin", "refs/tags/v1"],
    );
    repository::require_published(path, url, "v1", &object.object)
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
    assert!(
        repository::require_published(path, url, "v1", &object.object)
            .await
            .is_err()
    );
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
        let error = client::create(&recorded, &enterprise(), &release_request("v1", "", true))
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "Unexpected release URL from GitHub");
    }
}

#[tokio::test]
async fn release_can_be_explicitly_published_instead_of_saved_as_draft() {
    let recorded = Recorded::new(&json!({"method":"POST","path":"/repos/admin/skein-fixture-api/releases","status":201,"body":{"id":33,"html_url":"https://gitint.company.com/admin/skein-fixture-api/releases/33","draft":false}}).to_string());
    let release = client::create(&recorded, &enterprise(), &release_request("v1", "", false))
        .await
        .unwrap();
    assert!(!release.draft);
    assert_eq!(recorded.bodies.borrow()[0]["draft"], false);
}

struct Injected {
    transport: Recorded,
    resolved: RefCell<Vec<String>>,
    connected: RefCell<usize>,
    failures: HashMap<String, String>,
}

impl Injected {
    fn new() -> Self {
        Self {
            transport: Recorded::new(include_str!("fixtures/enterprise.json")),
            resolved: RefCell::default(),
            connected: RefCell::default(),
            failures: HashMap::new(),
        }
    }
}

impl Connection for Injected {
    type Transport<'a> = Recorded;

    async fn resolve(&self, path: &str, remote: &str) -> Result<(Repository, String), PullsError> {
        self.resolved.borrow_mut().push(remote.to_string());
        if let Some(error) = self.failures.get(remote) {
            return Err(error.clone().into());
        }
        let url = git(Path::new(path), &["remote", "get-url", "--push", remote]);
        Ok((enterprise(), url))
    }

    async fn load_source(
        &self,
        _path: &str,
        _repo: &Repository,
    ) -> Result<crate::settings::Source, PullsError> {
        Ok(serde_json::from_value(json!({
            "id": "fixture", "name": "Fixture", "kind": "ghe", "host": "gitint.company.com"
        }))
        .unwrap())
    }

    async fn connect<'a>(
        &'a self,
        _source: &'a crate::settings::Source,
        _host: &'a str,
    ) -> Result<Self::Transport<'a>, PullsError> {
        *self.connected.borrow_mut() += 1;
        Ok(self.transport.clone())
    }
}

fn request<'a>(fixture: &'a Fixture, remote: Option<&'a str>) -> ReleaseRequest<'a> {
    ReleaseRequest {
        path: fixture.work.to_str().unwrap(),
        tag: "v1",
        notes: "Release notes",
        draft: true,
        remote,
    }
}

fn add_upstream(fixture: &Fixture) -> PathBuf {
    let remote = fixture.root.join("upstream.git");
    std::fs::create_dir_all(&remote).unwrap();
    git(&remote, &["init", "--quiet", "--bare"]);
    git(
        &fixture.work,
        &["remote", "add", "upstream", remote.to_str().unwrap()],
    );
    remote
}

#[tokio::test]
async fn release_peels_the_annotated_tag_commit_instead_of_current_head() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    let commit = git(&fixture.work, &["rev-parse", "HEAD"]);
    git(
        &fixture.work,
        &["commit", "--quiet", "--allow-empty", "-m", "next commit"],
    );
    let tag = repository::annotated_object(fixture.work.to_str().unwrap(), "v1")
        .await
        .unwrap();
    assert_ne!(commit, git(&fixture.work, &["rev-parse", "HEAD"]));
    assert_eq!(tag.commit, commit);
    assert_eq!(tag.object, git(&fixture.work, &["rev-parse", "v1"]));
    assert_ne!(tag.object, tag.commit);
}

#[tokio::test]
async fn release_checks_publication_before_connecting_or_posting() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    let connection = Injected::new();
    let error = create_with_connection(&connection, request(&fixture, Some("origin")))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not on the remote"));
    assert_eq!(*connection.connected.borrow(), 0);
    assert!(connection.transport.bodies.borrow().is_empty());
    git(
        &fixture.work,
        &["push", "--quiet", "origin", "refs/tags/v1"],
    );
    git(
        &fixture.remote,
        &[
            "update-ref",
            "refs/tags/v1",
            &git(&fixture.work, &["rev-parse", "HEAD"]),
        ],
    );
    let error = create_with_connection(&connection, request(&fixture, Some("origin")))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("expected object"));
    assert_eq!(*connection.connected.borrow(), 0);
    assert!(connection.transport.bodies.borrow().is_empty());
    git(
        &fixture.work,
        &["push", "--quiet", "--force", "origin", "refs/tags/v1"],
    );
    create_with_connection(&connection, request(&fixture, Some("origin")))
        .await
        .unwrap();
    assert_eq!(*connection.connected.borrow(), 1);
    assert_eq!(connection.transport.bodies.borrow().len(), 1);
    assert_eq!(
        connection.transport.bodies.borrow()[0]["target_commitish"],
        git(&fixture.work, &["rev-parse", "v1^{commit}"])
    );
}

#[tokio::test]
async fn release_refuses_an_unauthenticated_connection_without_posting() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    git(
        &fixture.work,
        &["push", "--quiet", "origin", "refs/tags/v1"],
    );
    let mut connection = Injected::new();
    connection.transport.authenticated = false;
    let error = create_with_connection(&connection, request(&fixture, None))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Store a GitHub token"));
    assert_eq!(*connection.connected.borrow(), 1);
    assert!(connection.transport.bodies.borrow().is_empty());
    assert!(connection.transport.exchange.borrow().is_some());
}

#[tokio::test]
async fn release_uses_only_the_requested_remote_even_when_origin_is_published() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    add_upstream(&fixture);
    for remote in ["origin", "upstream"] {
        git(&fixture.work, &["push", "--quiet", remote, "refs/tags/v1"]);
    }
    let connection = Injected::new();
    create_with_connection(&connection, request(&fixture, Some("upstream")))
        .await
        .unwrap();
    assert_eq!(*connection.resolved.borrow(), ["upstream"]);
    assert_eq!(connection.transport.bodies.borrow().len(), 1);
}

#[tokio::test]
async fn release_falls_back_across_remotes_only_when_none_is_requested() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    add_upstream(&fixture);
    git(
        &fixture.work,
        &["push", "--quiet", "upstream", "refs/tags/v1"],
    );
    let connection = Injected::new();
    create_with_connection(&connection, request(&fixture, None))
        .await
        .unwrap();
    assert_eq!(*connection.resolved.borrow(), ["origin", "upstream"]);
    assert_eq!(*connection.connected.borrow(), 1);
    assert_eq!(connection.transport.bodies.borrow().len(), 1);
}

#[tokio::test]
async fn release_preserves_the_requested_remote_error_without_falling_back() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    add_upstream(&fixture);
    git(
        &fixture.work,
        &["push", "--quiet", "upstream", "refs/tags/v1"],
    );
    let connection = Injected::new();
    let error = create_with_connection(&connection, request(&fixture, Some("origin")))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not on the remote"));
    assert_eq!(*connection.resolved.borrow(), ["origin"]);
    assert!(connection.transport.bodies.borrow().is_empty());
}

#[tokio::test]
async fn release_preserves_the_first_remote_error_when_all_remotes_fail() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    add_upstream(&fixture);
    let mut connection = Injected::new();
    connection
        .failures
        .insert("upstream".into(), "upstream failure".into());
    let error = create_with_connection(&connection, request(&fixture, None))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not on the remote"));
    assert_eq!(*connection.resolved.borrow(), ["origin", "upstream"]);
    assert!(connection.transport.bodies.borrow().is_empty());
}

#[tokio::test]
async fn release_validates_requested_remote_names_and_configuration() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    let connection = Injected::new();
    for remote in ["", "--all", "origin\nupstream", "missing"] {
        let error = create_with_connection(&connection, request(&fixture, Some(remote)))
            .await
            .unwrap_err();
        let expected = if remote == "missing" {
            "There is no remote named missing"
        } else {
            "Invalid remote name"
        };
        assert_eq!(error.to_string(), expected);
    }
    assert!(connection.resolved.borrow().is_empty());
    assert_eq!(*connection.connected.borrow(), 0);
    assert!(connection.transport.bodies.borrow().is_empty());
}

#[tokio::test]
async fn release_remote_fallback_keeps_origin_first() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = fixture();
    for remote in ["zeta", "alpha"] {
        git(
            &fixture.work,
            &["remote", "add", remote, fixture.remote.to_str().unwrap()],
        );
    }
    assert_eq!(
        repository::remotes(fixture.work.to_str().unwrap(), None)
            .await
            .unwrap(),
        ["origin", "alpha", "zeta"]
    );
}
