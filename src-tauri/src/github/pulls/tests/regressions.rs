use super::*;
use crate::github::pulls::{source, PullsError};

#[tokio::test]
async fn latest_dismissed_review_removes_previous_approval_and_changes_requested() {
    for fixture in [
        include_str!("../fixtures/changes_approved_dismissed.json"),
        include_str!("../fixtures/approved_changes_dismissed.json"),
    ] {
        let recorded = Recorded::new(fixture);
        let pull = client::pull_for_branch(&recorded, &branch())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pull.review_state, ReviewState::ReviewRequired);
        recorded.exhausted();
    }
}

#[tokio::test]
async fn dismissed_reviewer_does_not_remove_another_reviewers_approval() {
    let recorded = Recorded::new(include_str!("../fixtures/changes_approved_dismissed.json"));
    recorded.exchanges.borrow_mut()[2].body.as_array_mut().unwrap().extend([
        json!({"id":10,"user":{"id":2},"state":"APPROVED","submitted_at":"2026-10-06T12:00:00Z"}),
        json!({"id":11,"user":{"id":2},"state":"COMMENTED","submitted_at":"2026-10-06T12:01:00Z"}),
        json!({"id":12,"user":{"id":2},"state":"PENDING","submitted_at":null}),
    ]);
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.review_state, ReviewState::Approved);
    recorded.exhausted();
}

#[tokio::test]
async fn closed_and_merged_pulls_with_a_different_head_are_hidden() {
    for fixture in [
        include_str!("../fixtures/closed.json"),
        include_str!("../fixtures/merged.json"),
    ] {
        let recorded = Recorded::new(fixture);
        recorded.exchanges.borrow_mut().truncate(3);
        let mut branch = branch();
        branch.sha = "c".repeat(40);
        assert!(client::pull_for_branch(&recorded, &branch)
            .await
            .unwrap()
            .is_none());
        recorded.exhausted();
    }
}

#[tokio::test]
async fn an_open_pull_reports_commits_missing_from_its_remote_head() {
    let recorded = Recorded::new(include_str!("../fixtures/approved.json"));
    let mut branch = branch();
    branch.sha = "c".repeat(40);
    let pull = client::pull_for_branch(&recorded, &branch)
        .await
        .unwrap()
        .unwrap();
    let value = serde_json::to_value(pull).unwrap();
    assert_eq!(value["hasUnpushedCommits"], true);
    assert_eq!(value["targetRepo"], "admin/skein-fixture-api");
    recorded.exhausted();
}

#[tokio::test]
async fn creation_compares_the_api_branch_commit_with_the_local_tip() {
    for sha in ["a".repeat(40), "c".repeat(40)] {
        let recorded = Recorded::new(include_str!("../fixtures/created.json"));
        let mut branch = branch();
        branch.sha = sha.clone();
        let pull = client::open_pull_request(&recorded, &branch, &request())
            .await
            .unwrap();
        assert_eq!(pull.has_unpushed_commits, sha != "a".repeat(40));
        assert_eq!(pull.target_repo, "admin/skein-fixture-api");
        recorded.exhausted();
    }
}

#[tokio::test]
async fn branch_authentication_failures_do_not_suggest_pushing_or_post_a_pull() {
    for status in [401, 403, 429, 500] {
        let recorded = Recorded::new(include_str!("../fixtures/created.json"));
        recorded.exchanges.borrow_mut()[0].status = status;
        recorded.exchanges.borrow_mut().truncate(1);
        let error = client::open_pull_request(&recorded, &branch(), &request())
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("push it before"));
        recorded.exhausted();
    }
}

#[tokio::test]
async fn fork_lookup_and_creation_use_the_parent_and_fork_owner() {
    let recorded = Recorded::new(include_str!("../fixtures/fork_approved.json"));
    let pull = client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pull.target_repo, "upstream/skein-fixture-api");
    recorded.exhausted();
    for fixture in [
        include_str!("../fixtures/fork_created.json"),
        include_str!("../fixtures/fork_already_exists.json"),
    ] {
        let recorded = Recorded::new(fixture);
        let pull = client::open_pull_request(&recorded, &branch(), &request())
            .await
            .unwrap();
        assert_eq!(pull.target_repo, "upstream/skein-fixture-api");
        assert_eq!(recorded.bodies.borrow()[0]["head"], "admin:feature/login");
        recorded.exhausted();
    }
}

#[tokio::test]
async fn upstream_branch_name_is_used_for_publication_lookup_and_posting() {
    let recorded = Recorded::new(include_str!("../fixtures/created.json"));
    recorded.exchanges.borrow_mut()[0].path =
        "/repos/admin/skein-fixture-api/branches/remote%2Ffeature".into();
    let mut branch = branch();
    branch.head = "remote/feature".into();
    client::open_pull_request(&recorded, &branch, &request())
        .await
        .unwrap();
    assert_eq!(recorded.bodies.borrow()[0]["head"], "remote/feature");
    recorded.exhausted();
}

#[test]
fn enterprise_remotes_accept_https_ssh_ports_and_scp_on_the_candidate_host() {
    for host in ["gitext.company.com", "gitint.company.com"] {
        for url in [
            format!("https://{host}/admin/repo.git/"),
            format!("ssh://git@{host}:2222/admin/repo.git"),
            format!("git@{host}:admin/repo.git"),
        ] {
            let repo = repository::parse_remote(&url, host).unwrap();
            assert_eq!(repo.host, host);
            assert_eq!(repo.owner, "admin");
            assert_eq!(repo.name, "repo");
            assert!(repository::parse_remote(&url, "github.com").is_err());
        }
    }
}

#[test]
fn each_configured_host_resolves_its_own_source() {
    let sources: Vec<crate::settings::Source> =
        serde_json::from_str(include_str!("../fixtures/enterprise_sources.json")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/github");
    for configured in &sources {
        let mut settings = source_settings();
        settings.sources = sources.clone();
        settings.workspace["sets"][0]["items"][0]["repoId"] =
            format!("{}:admin/skein-fixture-api", configured.id).into();
        settings.workspace["sets"][0]["items"][0]["url"] =
            format!("https://{}/admin/skein-fixture-api.git", configured.host).into();
        let mut repo = repo();
        repo.host = configured.host.clone();
        assert_eq!(
            source::for_path(&settings, &path, &repo).unwrap().id,
            configured.id
        );
    }
}

#[test]
fn an_unknown_remote_host_names_the_source_to_add() {
    let repo = repository::parse_remote(
        "ssh://git@unknown.company.com:2222/admin/skein-fixture-api.git",
        "unknown.company.com",
    )
    .unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/github");
    assert_eq!(
        source::for_path(&source_settings(), &path, &repo).unwrap_err(),
        "Add a source for unknown.company.com"
    );
}

#[test]
fn rate_limit_command_errors_serialize_rfc3339_and_safe_messages() {
    let reset_at = chrono::DateTime::from_timestamp(1_791_302_400, 0).unwrap();
    let value = serde_json::to_value(PullsError::from(Error::RateLimited { reset_at })).unwrap();
    assert_eq!(value["kind"], "rateLimited");
    assert_eq!(value["resetAt"], reset_at.to_rfc3339());
    assert!(chrono::DateTime::parse_from_rfc3339(value["resetAt"].as_str().unwrap()).is_ok());
    let value =
        serde_json::to_value(PullsError::from("Cannot read the branch".to_string())).unwrap();
    assert_eq!(
        value,
        json!({"kind":"message","message":"Cannot read the branch"})
    );
}

#[test]
fn enterprise_responses_without_rate_limit_headers_decode_or_return_permission_errors() {
    for status in [200, 403, 429] {
        let response = Response {
            status,
            headers: HeaderMap::new(),
            body: b"{}".to_vec(),
            next: false,
        };
        let result = response.decode::<Value>();
        if status == 200 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(Error::Http { status: actual, .. }) if actual == status));
        }
    }
}

#[tokio::test]
async fn enterprise_pull_urls_are_preserved_for_lookup_and_creation() {
    let recorded = Recorded::new(include_str!("../fixtures/enterprise_approved.json"));
    let mut branch = branch();
    branch.repo.host = "gitext.company.com".into();
    let pull = client::pull_for_branch(&recorded, &branch)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pull.url,
        "https://gitext.company.com/admin/skein-fixture-api/pull/28"
    );
    recorded.exhausted();
    let recorded = Recorded::new(include_str!("../fixtures/enterprise_created.json"));
    let pull = client::open_pull_request(&recorded, &branch, &request())
        .await
        .unwrap();
    assert_eq!(
        pull.url,
        "https://gitext.company.com/admin/skein-fixture-api/pull/28"
    );
    recorded.exhausted();
}

#[tokio::test]
async fn equal_head_and_base_names_are_allowed_only_across_repositories() {
    let mut request = request();
    request.base = request.head.clone();
    let recorded = Recorded::new(include_str!("../fixtures/fork_created.json"));
    client::open_pull_request(&recorded, &branch(), &request)
        .await
        .unwrap();
    recorded.exhausted();
    let recorded = Recorded::new(include_str!("../fixtures/created.json"));
    recorded.exchanges.borrow_mut().truncate(2);
    let error = client::open_pull_request(&recorded, &branch(), &request)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("different from the head"));
    recorded.exhausted();
}

#[tokio::test]
async fn only_a_200_branch_response_counts_as_published() {
    let recorded = Recorded::new(include_str!("../fixtures/created.json"));
    recorded.exchanges.borrow_mut()[0].status = 201;
    recorded.exchanges.borrow_mut().truncate(1);
    assert!(client::open_pull_request(&recorded, &branch(), &request())
        .await
        .unwrap_err()
        .to_string()
        .contains("Unexpected branch response"));
    recorded.exhausted();
}

#[test]
fn source_resolution_preserves_identity_when_sources_share_a_host() {
    let mut settings = source_settings();
    let mut second = settings.sources[0].clone();
    second.id = "second-fixture".into();
    settings.sources.push(second);
    let mut item = settings.workspace["sets"][0]["items"][0].clone();
    item["id"] = "other-fixture-item".into();
    settings.workspace["sets"][0]["items"][0]["repoId"] =
        "second-fixture:admin/skein-fixture-api".into();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/github");
    assert_eq!(
        source::for_path(&settings, &path, &repo()).unwrap().id,
        "second-fixture"
    );
    settings.workspace["sets"][0]["items"]
        .as_array_mut()
        .unwrap()
        .push(item);
    assert!(source::for_path(&settings, &path, &repo())
        .unwrap_err()
        .contains("several GitHub sources"));
}

#[tokio::test]
async fn a_matching_older_closed_pull_does_not_replace_the_latest_closed_pull() {
    let recorded = Recorded::new(include_str!("../fixtures/closed.json"));
    {
        let mut exchanges = recorded.exchanges.borrow_mut();
        let mut older = exchanges[2].body[0].clone();
        older["number"] = 27.into();
        older["closed_at"] = "2026-10-05T12:00:00Z".into();
        exchanges[2].body[0]["head"]["sha"] = "c".repeat(40).into();
        exchanges[2].body.as_array_mut().unwrap().push(older);
        exchanges.truncate(3);
    }
    assert!(client::pull_for_branch(&recorded, &branch())
        .await
        .unwrap()
        .is_none());
    recorded.exhausted();
}
