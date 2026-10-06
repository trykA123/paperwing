use super::super::http::{Error, Response};
use super::client::{self, Transport};
use super::model::{ChecksState, OpenPullRequest, PullState, ReviewState};
use super::repository::{self, Branch, Repository};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

#[derive(Deserialize)]
struct Exchange {
    method: String,
    path: String,
    status: u16,
    #[serde(default)]
    headers: HashMap<String, String>,
    body: Value,
    #[serde(default)]
    next: bool,
}

struct Recorded {
    exchanges: RefCell<VecDeque<Exchange>>,
    bodies: RefCell<Vec<Value>>,
}

impl Recorded {
    fn new(fixture: &str) -> Self {
        Self {
            exchanges: RefCell::new(serde_json::from_str(fixture).unwrap()),
            bodies: RefCell::default(),
        }
    }

    fn exhausted(&self) {
        assert!(self.exchanges.borrow().is_empty());
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
            .exchanges
            .borrow_mut()
            .pop_front()
            .expect("Unexpected HTTP request");
        assert_eq!(method.as_str(), exchange.method);
        assert_eq!(path, exchange.path);
        if let Some(body) = body {
            self.bodies.borrow_mut().push(body);
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
            body: serde_json::to_vec(&exchange.body).unwrap(),
            next: exchange.next,
        })
    }
}

fn repo() -> Repository {
    repository::parse_remote("git@github.com:admin/skein-fixture-api.git", "github.com").unwrap()
}

fn branch() -> Branch {
    Branch {
        repo: repo(),
        head: "feature/login".into(),
        sha: "a".repeat(40),
    }
}

fn request() -> OpenPullRequest {
    serde_json::from_value(json!({"head":"feature/login","base":"main","title":"Add login","body":"Add a login flow."})).unwrap()
}

#[tokio::test]
async fn open_pull_has_approval_and_combined_green_checks() {
    let recorded = Recorded::new(include_str!("fixtures/approved.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.number, 28);
    assert_eq!(pull.title, "Add login");
    assert_eq!(
        pull.url,
        "https://github.com/admin/skein-fixture-api/pull/28"
    );
    assert_eq!(pull.base, "main");
    assert_eq!(pull.head_sha, "a".repeat(40));
    assert_eq!(pull.state, PullState::Open);
    assert_eq!(pull.review_state, ReviewState::Approved);
    assert_eq!(pull.checks.state, ChecksState::Success);
    assert_eq!(
        (
            pull.checks.success,
            pull.checks.failure,
            pull.checks.pending,
            pull.checks.total
        ),
        (2, 0, 0, 2)
    );
    let value = serde_json::to_value(pull).unwrap();
    assert_eq!(value["reviewState"], "approved");
    assert_eq!(value["headSha"], "a".repeat(40));
    recorded.exhausted();
}

#[tokio::test]
async fn draft_pull_has_review_required_and_no_checks() {
    let recorded = Recorded::new(include_str!("fixtures/draft.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.state, PullState::Draft);
    assert_eq!(pull.review_state, ReviewState::ReviewRequired);
    assert_eq!(pull.checks.state, ChecksState::None);
    assert_eq!(pull.checks.total, 0);
    recorded.exhausted();
}

#[tokio::test]
async fn latest_merged_pull_is_returned_when_no_open_pull_exists() {
    let recorded = Recorded::new(include_str!("fixtures/merged.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.state, PullState::Merged);
    assert_eq!(pull.review_state, ReviewState::None);
    recorded.exhausted();
}

#[tokio::test]
async fn latest_unmerged_closed_pull_is_returned_as_closed() {
    let recorded = Recorded::new(include_str!("fixtures/closed.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.state, PullState::Closed);
    recorded.exhausted();
}

#[tokio::test]
async fn closed_pull_selection_uses_closing_time_across_pages() {
    let recorded = Recorded::new(include_str!("fixtures/closed.json"));
    {
        let mut exchanges = recorded.exchanges.borrow_mut();
        let mut older = exchanges[2].body[0].clone();
        older["number"] = 27.into();
        older["closed_at"] = "2026-10-05T12:00:00Z".into();
        let newest = exchanges[2].body[0].clone();
        exchanges[2].body = json!([older]);
        exchanges[2].next = true;
        let path = exchanges[2].path.replace("&page=1", "&page=2");
        exchanges.insert(
            3,
            serde_json::from_value(
                json!({"method":"GET","path":path,"status":200,"body":[newest]}),
            )
            .unwrap(),
        );
    }
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.number, 28);
    recorded.exhausted();
}

#[tokio::test]
async fn no_pull_returns_none_without_fetching_reviews_or_checks() {
    let recorded = Recorded::new(include_str!("fixtures/none.json"));
    assert!(client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .is_none());
    recorded.exhausted();
}

#[tokio::test]
async fn later_approval_supersedes_changes_from_the_same_reviewer_across_pages() {
    let recorded = Recorded::new(include_str!("fixtures/superseded.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.review_state, ReviewState::Approved);
    recorded.exhausted();
}

#[tokio::test]
async fn changes_requested_by_another_reviewer_wins_over_approval() {
    let recorded = Recorded::new(include_str!("fixtures/approved.json"));
    recorded.exchanges.borrow_mut()[2].body.as_array_mut().unwrap().push(json!({"id":5,"state":"CHANGES_REQUESTED","user":{"id":2},"submitted_at":"2026-10-06T12:00:00Z"}));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.review_state, ReviewState::ChangesRequested);
    assert_eq!(
        serde_json::to_value(pull).unwrap()["reviewState"],
        "changesRequested"
    );
    recorded.exhausted();
}

#[tokio::test]
async fn pending_check_runs_win_over_successful_commit_statuses() {
    let recorded = Recorded::new(include_str!("fixtures/mixed_pending.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.checks.state, ChecksState::Pending);
    assert_eq!(
        (
            pull.checks.success,
            pull.checks.failure,
            pull.checks.pending,
            pull.checks.total
        ),
        (2, 0, 1, 3)
    );
    recorded.exhausted();
}

#[tokio::test]
async fn failed_commit_status_wins_over_pending_and_successful_checks() {
    let recorded = Recorded::new(include_str!("fixtures/mixed_failure.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.checks.state, ChecksState::Failure);
    assert_eq!(
        (
            pull.checks.success,
            pull.checks.failure,
            pull.checks.pending,
            pull.checks.total
        ),
        (2, 1, 1, 4)
    );
    recorded.exhausted();
}

#[tokio::test]
async fn latest_status_per_context_supersedes_a_historical_failure() {
    let recorded = Recorded::new(include_str!("fixtures/status_superseded.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.checks.state, ChecksState::Success);
    assert_eq!(pull.checks.total, 2);
    recorded.exhausted();
}

#[tokio::test]
async fn check_runs_and_statuses_follow_pagination() {
    let recorded = Recorded::new(include_str!("fixtures/mixed_pending.json"));
    {
        let mut exchanges = recorded.exchanges.borrow_mut();
        exchanges[3].next = true;
        let mut page: Exchange = serde_json::from_value(json!({"method":"GET","path":"","status":200,"body":{"check_runs":[{"status":"completed","conclusion":"failure"}]}})).unwrap();
        page.path = exchanges[3].path.replace("&page=1", "&page=2");
        exchanges.insert(4, page);
        exchanges[5].next = true;
        let path = exchanges[5].path.replace("&page=1", "&page=2");
        exchanges.push_back(serde_json::from_value(json!({"method":"GET","path":path,"status":200,"body":{"statuses":[{"id":4,"context":"second","state":"pending"}]}})).unwrap());
    }
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.checks.state, ChecksState::Failure);
    assert_eq!(
        (pull.checks.failure, pull.checks.pending, pull.checks.total),
        (1, 2, 5)
    );
    recorded.exhausted();
}

#[tokio::test]
async fn pagination_cap_fails_instead_of_returning_an_incomplete_summary() {
    let recorded = Recorded::new(include_str!("fixtures/draft.json"));
    let metadata = recorded.exchanges.borrow_mut().pop_front().unwrap();
    let first = recorded.exchanges.borrow_mut().pop_front().unwrap();
    recorded.exchanges.borrow_mut().clear();
    recorded.exchanges.borrow_mut().push_back(metadata);
    recorded.exchanges.borrow_mut().push_back(first);
    for page in 1..=20 {
        recorded.exchanges.borrow_mut().push_back(serde_json::from_value(json!({"method":"GET","path":format!("/repos/admin/skein-fixture-api/pulls/28/reviews?per_page=100&page={page}"),"status":200,"body":[],"next":true})).unwrap());
    }
    let error = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("pagination limit"));
    recorded.exhausted();
}

#[tokio::test]
async fn rate_limit_errors_carry_utc_reset_without_retrying_or_disclosing_response_body() {
    for (status, remaining) in [(403, true), (429, true), (403, false)] {
        let recorded = Recorded::new(include_str!("fixtures/rate_limit.json"));
        recorded.exchanges.borrow_mut()[1].status = status;
        if !remaining {
            recorded.exchanges.borrow_mut()[1]
                .headers
                .remove("x-ratelimit-remaining");
        }
        let error = client::pull_for_branch(&recorded, &branch())
            .await
            .unwrap_err();
        assert!(
            matches!(&error, Error::RateLimited { reset_at } if reset_at.timestamp() == 1_791_302_400)
        );
        assert!(!error.to_string().contains("synthetic-token"));
        assert!(error.to_string().contains("UTC"));
        recorded.exhausted();
    }
}

#[test]
fn secondary_rate_limit_uses_server_date_and_retry_after() {
    let mut headers = HeaderMap::new();
    headers.insert("retry-after", HeaderValue::from_static("60"));
    headers.insert(
        "date",
        HeaderValue::from_static("Tue, 06 Oct 2026 12:00:00 GMT"),
    );
    let error = Response {
        status: 429,
        headers,
        body: vec![],
        next: false,
    }
    .decode::<Value>()
    .err()
    .unwrap();
    assert!(
        matches!(error, Error::RateLimited { reset_at } if reset_at.to_rfc3339() == "2026-10-06T12:01:00+00:00")
    );
}

#[test]
fn ordinary_permission_denial_is_not_misreported_as_a_rate_limit() {
    let mut headers = HeaderMap::new();
    headers.insert("x-ratelimit-remaining", HeaderValue::from_static("4999"));
    headers.insert("x-ratelimit-reset", HeaderValue::from_static("1791302400"));
    let error = Response {
        status: 403,
        headers,
        body: vec![],
        next: false,
    }
    .decode::<Value>()
    .err()
    .unwrap();
    assert!(matches!(error, Error::Http { status: 403, .. }));
    assert!(error.to_string().contains("permissions"));
}

#[tokio::test]
async fn already_exists_422_returns_the_existing_pull() {
    let recorded = Recorded::new(include_str!("fixtures/already_exists.json"));
    let pull = client::open_pull_request(&recorded, &branch(), &request())
        .await
        .unwrap();
    assert_eq!(pull.number, 28);
    assert_eq!(
        pull.url,
        "https://github.com/admin/skein-fixture-api/pull/28"
    );
    assert_eq!(recorded.bodies.borrow()[0]["draft"], true);
    recorded.exhausted();
}

#[tokio::test]
async fn creation_sends_draft_by_default_and_respects_explicit_false() {
    for draft in [true, false] {
        let recorded = Recorded::new(include_str!("fixtures/created.json"));
        let mut request = request();
        request.draft = draft;
        let pull = client::open_pull_request(&recorded, &branch(), &request)
            .await
            .unwrap();
        assert_eq!(pull.number, 28);
        assert_eq!(
            recorded.bodies.borrow()[0],
            json!({"head":"feature/login","base":"main","title":"Add login","body":"Add a login flow.","draft":draft})
        );
        assert_eq!(
            serde_json::to_value(pull).unwrap()["url"],
            "https://github.com/admin/skein-fixture-api/pull/28"
        );
        recorded.exhausted();
    }
}

#[tokio::test]
async fn unpublished_branch_is_rejected_after_the_authenticated_branch_check() {
    let recorded = Recorded::new(
        r#"[{"method":"GET","path":"/repos/admin/skein-fixture-api/branches/feature%2Flogin","status":404,"body":{}}]"#,
    );
    let error = client::open_pull_request(&recorded, &branch(), &request())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("push it before"));
    recorded.exhausted();
}

#[test]
fn parses_https_ssh_git_suffix_and_trailing_slash() {
    for url in [
        "https://github.com/admin/skein-fixture-api",
        "https://github.com/admin/skein-fixture-api.git",
        "https://github.com/admin/skein-fixture-api.git/",
        "https://github.com/admin/skein-fixture-api/",
        "git@github.com:admin/skein-fixture-api.git",
        "ssh://git@github.com/admin/skein-fixture-api.git",
        "ssh://git@github.com:22/admin/skein-fixture-api.git/",
    ] {
        let repo = repository::parse_remote(url, "github.com").unwrap();
        assert_eq!(repo.owner, "admin");
        assert_eq!(repo.name, "skein-fixture-api");
    }
}

#[test]
fn rejects_non_github_hosts_and_unsafe_or_malformed_urls_without_echoing_them() {
    for url in [
        "https://gitlab.com/admin/repo.git",
        "git@gitlab.com:admin/repo.git",
        "ssh://git@github.com.invalid/admin/repo.git",
        "https://github.com/admin/repo/extra",
        "https://github.com/admin/repo?token=synthetic-token",
        "https://admin:synthetic-token@github.com/admin/repo",
        "https://github.com/admin/../repo",
        "http://github.com/admin/repo",
        "file:///admin/repo",
        "git@github.com:admin",
        "https://github.com/admin/.git",
    ] {
        let error = repository::parse_remote(url, "github.com").unwrap_err();
        assert!(error.to_string().contains("GitHub remote"));
        assert!(!error.to_string().contains("synthetic-token"));
    }
}

#[test]
fn invalid_creation_arguments_are_rejected() {
    for (field, value) in [
        ("title", ""),
        ("title", "title\nnewline"),
        ("head", "-bad"),
        ("body", "bad\0body"),
    ] {
        let mut value_json = serde_json::to_value(request()).unwrap();
        value_json[field] = value.into();
        assert!(serde_json::from_value::<OpenPullRequest>(value_json)
            .unwrap()
            .validate()
            .is_err());
    }
}

#[test]
fn http_errors_show_safe_auth_access_and_validation_messages() {
    for (status, expected) in [
        (401, "stored GitHub token"),
        (403, "permissions"),
        (404, "not found"),
        (422, "branches and title"),
        (500, "HTTP 500"),
    ] {
        let response = Response {
            status,
            headers: HeaderMap::new(),
            body: br#"{"message":"synthetic-token"}"#.to_vec(),
            next: false,
        };
        let error = response.decode::<Value>().err().unwrap();
        assert!(error.to_string().contains(expected));
        assert!(!error.to_string().contains("synthetic-token"));
    }
}

#[tokio::test]
async fn transport_failure_is_returned_without_additional_requests() {
    struct Offline;
    impl Transport for Offline {
        async fn send(&self, _: Method, _: &str, _: Option<Value>) -> Result<Response, Error> {
            Err(Error::Message(
                "Cannot reach github.com; check your connection".into(),
            ))
        }
    }
    let error = client::pull_for_branch(&Offline, &branch())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("check your connection"));
}

fn source_settings() -> crate::settings::Settings {
    serde_json::from_value(json!({
        "sources": [{"id":"fixture-source","kind":"github","host":"github.com","name":"Fixture"}],
        "workspace": {"root":format!("{}/src", env!("CARGO_MANIFEST_DIR")),"layout":"flat","sets":[{
            "id":"fixture-set","name":"Fixture","items":[{
                "id":"fixture-item","repoId":"fixture-source:admin/skein-fixture-api","name":"github",
                "url":"https://github.com/admin/skein-fixture-api.git","org":"admin","ref":{"type":"branch","name":"main"}
            }]
        }]}
    })).unwrap()
}

#[test]
fn registered_path_selects_its_stored_source_without_changing_configuration() {
    let mut settings = source_settings();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/github");
    let source = super::source::for_path(&settings, &path, &repo()).unwrap();
    assert_eq!(source.id, "fixture-source");
    settings.sources[0].kind = "manual".into();
    settings.sources[0].host.clear();
    settings.sources[0].urls = vec!["https://github.com/admin/skein-fixture-api.git".into()];
    settings.sources[0].credential_managed = true;
    let source = super::source::for_path(&settings, &path, &repo()).unwrap();
    assert_eq!(
        serde_json::to_value(source).unwrap(),
        serde_json::to_value(&settings.sources[0]).unwrap()
    );
}

#[test]
fn source_resolution_rejects_changed_remotes_unregistered_paths_and_missing_sources() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/github");
    let mut settings = source_settings();
    let other = repository::parse_remote("https://github.com/admin/other", "github.com").unwrap();
    assert!(super::source::for_path(&settings, &path, &other)
        .unwrap_err()
        .contains("does not match"));
    assert!(
        super::source::for_path(&settings, &path.parent().unwrap().join("git"), &repo())
            .unwrap_err()
            .contains("not registered")
    );
    settings.sources.clear();
    assert!(super::source::for_path(&settings, &path, &repo())
        .unwrap_err()
        .contains("Add a source for github.com"));
}

mod regressions;
