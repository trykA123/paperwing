use super::http::GithubApi;
use super::{valid_name, GhLogin, GhRepo, Repo, RepoList, Source};

const MAX_PAGES: u32 = 100;
const OWNER_CONCURRENCY: usize = 4;

struct OwnerListing {
    repos: Vec<Repo>,
    warning: Option<String>,
}

pub(super) async fn authenticated_login(api: &impl GithubApi) -> Result<Option<String>, String> {
    if !api.authenticated() {
        return Ok(None);
    }
    let profile = api
        .get::<GhLogin>("/user")
        .await
        .map_err(|(_, reason)| reason)?
        .data;
    valid_name(&profile.login)?;
    Ok(Some(profile.login))
}

pub(super) async fn revalidate(
    source: &Source,
    request: &super::cache::ListingRequest,
    store: crate::store::Store,
    api: &impl GithubApi,
) -> Result<RepoList, String> {
    let login = authenticated_login(api).await?;
    let list = fetch_listing(source, api, login.as_deref()).await;
    request.finish(store, login, &list).await?;
    Ok(list)
}

pub(super) async fn fetch_listing(
    source: &Source,
    api: &impl GithubApi,
    login: Option<&str>,
) -> RepoList {
    let mut list = RepoList {
        fetched_at: super::cache::now(),
        ..Default::default()
    };
    let owners: Vec<&str> = source.orgs.iter().map(String::as_str).collect();
    let mut results = Vec::with_capacity(owners.len());
    for chunk in owners.chunks(OWNER_CONCURRENCY) {
        results.extend(
            futures_util::future::join_all(
                chunk
                    .iter()
                    .map(|owner| owner_result(source, owner, api, login)),
            )
            .await,
        );
    }
    for (owner, result) in results {
        match result {
            Ok(found) => {
                list.repos.extend(found.repos);
                if let Some(warning) = found.warning {
                    list.warnings.push(format!("{owner}: {warning}"));
                    list.partial = true;
                }
            }
            Err(reason) => list.errors.push(format!("{owner}: {reason}")),
        }
    }
    list
}

async fn owner_result<'a>(
    source: &Source,
    owner: &'a str,
    api: &impl GithubApi,
    login: Option<&str>,
) -> (&'a str, Result<OwnerListing, String>) {
    (owner, list_owner(source, owner, api, login).await)
}

async fn list_owner(
    source: &Source,
    owner: &str,
    api: &impl GithubApi,
    login: Option<&str>,
) -> Result<OwnerListing, String> {
    valid_name(owner)?;
    let personal =
        api.authenticated() && login.is_some_and(|login| login.eq_ignore_ascii_case(owner));
    let mut route = if personal {
        "/user/repos?affiliation=owner&visibility=all".into()
    } else {
        format!("/orgs/{owner}/repos?type=all")
    };
    let mut repos = Vec::new();
    let mut page = 1;
    loop {
        let response = api
            .get::<Vec<GhRepo>>(&format!("{route}&per_page=100&page={page}"))
            .await;
        let batch = match response {
            Ok(batch) => batch,
            Err((404, _)) if !personal && page == 1 && route.starts_with("/orgs/") => {
                route = format!("/users/{owner}/repos?type=owner");
                continue;
            }
            Err((status, reason)) if !repos.is_empty() && is_transient_error(status, &reason) => {
                return Ok(partial(repos, &reason));
            }
            Err((_, reason)) => return Err(reason),
        };
        let more = batch.next || batch.data.len() == 100;
        for repo in batch.data {
            valid_name(&repo.owner.login)?;
            valid_name(&repo.name)?;
            if !repo.owner.login.eq_ignore_ascii_case(owner) {
                return Err("Repository owner does not match the configured owner".into());
            }
            repos.push(to_repo(source, repo));
        }
        if !more {
            return Ok(OwnerListing {
                repos,
                warning: None,
            });
        }
        if page >= MAX_PAGES {
            let reason = "Repository discovery is incomplete: pagination limit reached";
            return if repos.is_empty() {
                Err(reason.into())
            } else {
                Ok(partial(repos, reason))
            };
        }
        page += 1;
    }
}

fn partial(repos: Vec<Repo>, reason: &str) -> OwnerListing {
    let warning = format!("showing {} of more; GitHub returned {reason}", repos.len());
    OwnerListing {
        repos,
        warning: Some(warning),
    }
}

fn is_transient_error(status: u16, reason: &str) -> bool {
    (500..600).contains(&status)
        || (status == 0
            && (reason.starts_with("Cannot reach ") || reason == "Cannot read GitHub response"))
}

fn to_repo(source: &Source, repo: GhRepo) -> Repo {
    let org = repo.owner.login;
    Repo {
        id: format!("{}:{}/{}", source.id, org, repo.name),
        source: source.id.clone(),
        org,
        name: repo.name,
        description: repo.description.unwrap_or_default(),
        url: repo.ssh_url,
        default_branch: repo.default_branch.unwrap_or_default(),
        pushed_at: repo.pushed_at.unwrap_or_default(),
        archived: repo.archived,
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::Page;
    use super::*;
    use serde::de::DeserializeOwned;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    type Response = Result<Page<serde_json::Value>, (u16, String)>;

    struct Fixture {
        credential: bool,
        responses: RefCell<VecDeque<Response>>,
        paths: RefCell<Vec<String>>,
    }

    impl GithubApi for Fixture {
        fn authenticated(&self) -> bool {
            self.credential
        }
        async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, (u16, String)> {
            self.paths.borrow_mut().push(path.into());
            let response = self
                .responses
                .borrow_mut()
                .pop_front()
                .expect("fixture response");
            response.map(|response| Page {
                data: serde_json::from_value(response.data).unwrap(),
                next: response.next,
            })
        }
    }

    fn source(owner: &str) -> Source {
        serde_json::from_value(serde_json::json!({"id":"fixture","name":"admin","kind":"github","host":"github.com","orgs":[owner]})).unwrap()
    }

    fn fixture(responses: Vec<Response>) -> Fixture {
        Fixture {
            credential: true,
            responses: RefCell::new(responses.into()),
            paths: RefCell::new(Vec::new()),
        }
    }

    fn page(data: serde_json::Value, next: bool) -> Result<Page<serde_json::Value>, (u16, String)> {
        Ok(Page { data, next })
    }
    fn repo(owner: &str, name: &str, private: bool) -> serde_json::Value {
        serde_json::json!({"owner":{"login":owner},"name":name,"private":private,"ssh_url":format!("git@github.com:{owner}/{name}.git")})
    }

    #[tokio::test]
    async fn personal_private_and_public_repositories_use_paginated_owner_route() {
        let api = fixture(vec![
            page(serde_json::json!({"login":"Admin"}), false),
            page(serde_json::json!([repo("Admin", "private", true)]), true),
            page(serde_json::json!([repo("Admin", "public", false)]), false),
        ]);
        let login = authenticated_login(&api).await.unwrap();
        let list = fetch_listing(&source("admin"), &api, login.as_deref()).await;
        assert!(list.errors.is_empty());
        assert_eq!(
            list.repos
                .iter()
                .map(|repo| repo.id.as_str())
                .collect::<Vec<_>>(),
            ["fixture:Admin/private", "fixture:Admin/public"]
        );
        assert_eq!(
            *api.paths.borrow(),
            [
                "/user",
                "/user/repos?affiliation=owner&visibility=all&per_page=100&page=1",
                "/user/repos?affiliation=owner&visibility=all&per_page=100&page=2"
            ]
        );
    }

    #[tokio::test]
    async fn unrelated_owner_keeps_org_then_public_user_routes() {
        let api = fixture(vec![
            page(serde_json::json!({"login":"admin"}), false),
            Err((404, "Not found".into())),
            page(serde_json::json!([repo("another", "public", false)]), false),
        ]);
        let login = authenticated_login(&api).await.unwrap();
        let list = fetch_listing(&source("another"), &api, login.as_deref()).await;
        assert!(list.errors.is_empty());
        assert_eq!(list.repos[0].org, "another");
        assert_eq!(
            *api.paths.borrow(),
            [
                "/user",
                "/orgs/another/repos?type=all&per_page=100&page=1",
                "/users/another/repos?type=owner&per_page=100&page=1"
            ]
        );
    }

    #[tokio::test]
    async fn non_retryable_later_page_failures_discard_fetched_pages() {
        for status in [401, 403, 404] {
            let api = fixture(vec![
                page(serde_json::json!([repo("admin", "private", true)]), true),
                Err((status, "denied".into())),
            ]);
            let list = fetch_listing(&source("admin"), &api, Some("admin")).await;
            assert!(list.repos.is_empty());
            assert_eq!(list.errors, ["admin: denied"]);
            assert!(list.warnings.is_empty());
            assert!(!list.partial);
            assert_eq!(api.paths.borrow().len(), 2);
        }
        for status in [401, 403, 404] {
            let api = fixture(vec![Err((status, "denied".into()))]);
            let list = fetch_listing(&source("admin"), &api, Some("admin")).await;
            assert!(list.repos.is_empty());
            assert_eq!(list.errors, ["admin: denied"]);
            assert!(!list.partial);
        }
        let api = fixture(vec![Err((401, "expired".into()))]);
        assert_eq!(authenticated_login(&api).await.unwrap_err(), "expired");
        assert_eq!(*api.paths.borrow(), ["/user"]);
    }

    #[tokio::test]
    async fn unexpected_owners_are_rejected_and_anonymous_orgs_skip_profile() {
        let api = fixture(vec![page(
            serde_json::json!([repo("unrelated", "private", true)]),
            false,
        )]);
        let list = fetch_listing(&source("admin"), &api, Some("admin")).await;
        assert!(list.repos.is_empty());
        assert!(list.errors[0].contains("owner"));
        let mut api = fixture(vec![page(
            serde_json::json!([repo("organization", "public", false)]),
            false,
        )]);
        api.credential = false;
        assert_eq!(authenticated_login(&api).await.unwrap(), None);
        assert!(fetch_listing(&source("organization"), &api, None)
            .await
            .errors
            .is_empty());
        assert_eq!(
            *api.paths.borrow(),
            ["/orgs/organization/repos?type=all&per_page=100&page=1"]
        );
    }

    #[tokio::test]
    async fn page_limit_reports_incomplete_discovery() {
        let api = fixture(
            (0..MAX_PAGES)
                .map(|_| page(serde_json::json!([]), true))
                .collect(),
        );
        let list = fetch_listing(&source("admin"), &api, Some("admin")).await;
        assert!(list.repos.is_empty());
        assert!(list.errors[0].contains("incomplete"));
        assert_eq!(api.paths.borrow().len(), MAX_PAGES as usize);
    }

    fn many(owner: &str, from: usize, count: usize) -> serde_json::Value {
        (from..from + count)
            .map(|index| repo(owner, &format!("repo{index:04}"), false))
            .collect()
    }

    #[tokio::test]
    async fn a_failing_third_page_of_eight_keeps_two_hundred_repositories_and_warns() {
        let api = fixture(vec![
            page(many("big", 0, 100), true),
            page(many("big", 100, 100), true),
            Err((502, "bad gateway".into())),
        ]);
        let list = fetch_listing(&source("big"), &api, None).await;
        assert_eq!(list.repos.len(), 200);
        assert!(list.errors.is_empty());
        assert!(list.partial);
        assert_eq!(
            list.warnings,
            ["big: showing 200 of more; GitHub returned bad gateway"]
        );
    }

    #[tokio::test]
    async fn a_network_failure_after_a_page_returns_partial_results() {
        let api = fixture(vec![
            page(serde_json::json!([repo("big", "first", false)]), true),
            Err((0, "Cannot reach github.com".into())),
        ]);

        let list = fetch_listing(&source("big"), &api, None).await;

        assert_eq!(list.repos.len(), 1);
        assert!(list.errors.is_empty());
        assert!(list.partial);
        assert_eq!(
            list.warnings,
            ["big: showing 1 of more; GitHub returned Cannot reach github.com"]
        );
    }

    struct Concurrent {
        active: std::cell::Cell<usize>,
        peak: std::cell::Cell<usize>,
    }

    impl GithubApi for Concurrent {
        fn authenticated(&self) -> bool {
            false
        }
        async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, (u16, String)> {
            self.active.set(self.active.get() + 1);
            self.peak.set(self.peak.get().max(self.active.get()));
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            self.active.set(self.active.get() - 1);
            let owner = path.split('/').nth(2).unwrap().split('/').next().unwrap();
            let owner = owner.split('?').next().unwrap();
            Ok(Page {
                data: serde_json::from_value(serde_json::json!([repo(owner, "only", false)]))
                    .unwrap(),
                next: false,
            })
        }
    }

    #[tokio::test]
    async fn owners_are_listed_concurrently_up_to_four_and_keep_their_order() {
        let owners: Vec<String> = (0..9).map(|index| format!("owner{index}")).collect();
        let mut multi = source("owner0");
        multi.orgs = owners.clone();
        let api = Concurrent {
            active: Default::default(),
            peak: Default::default(),
        };
        let started = std::time::Instant::now();
        let list = fetch_listing(&multi, &api, None).await;
        assert_eq!(api.peak.get(), OWNER_CONCURRENCY);
        assert!(started.elapsed() < std::time::Duration::from_millis(30 * 9));
        assert!(list.errors.is_empty());
        assert_eq!(
            list.repos
                .iter()
                .map(|repo| repo.org.clone())
                .collect::<Vec<_>>(),
            owners
        );
    }

    fn store(fixture: &crate::platform::Fixture) -> crate::store::Store {
        crate::store::Store::open(&fixture.0.join("store.sqlite3"), &Default::default()).unwrap()
    }

    fn revalidation_source(id: &str) -> Source {
        Source {
            id: id.into(),
            ..source("admin")
        }
    }

    fn listing_of(owner: &str, name: &str, login: &str) -> Fixture {
        fixture(vec![
            page(serde_json::json!({"login":login}), false),
            page(serde_json::json!([repo(owner, name, true)]), false),
        ])
    }

    #[tokio::test]
    async fn login_change_replaces_the_stale_disk_rows() {
        let source = revalidation_source("revalidate-login");
        let scratch = crate::platform::Fixture::new("revalidate-login");
        let store = store(&scratch);
        let first = super::super::cache::ListingRequest::new(&source, false).unwrap();
        let api = listing_of("admin", "first", "admin");
        revalidate(&source, &first, store.clone(), &api)
            .await
            .unwrap();
        let again = super::super::cache::ListingRequest::new(&source, false).unwrap();
        let stale = again.read_stale(store.clone()).await.unwrap().unwrap();
        assert!(stale.stale);
        assert_eq!(stale.repos[0].name, "first");
        let api = listing_of("admin", "second", "Admin");
        let fresh = revalidate(&source, &again, store.clone(), &api)
            .await
            .unwrap();
        assert!(!fresh.stale);
        let stored = again.read_stale(store.clone()).await.unwrap().unwrap();
        assert_eq!(stored.repos[0].name, "second");
        store.close();
    }

    #[tokio::test]
    async fn offline_revalidation_keeps_the_stale_disk_rows() {
        let source = revalidation_source("revalidate-offline");
        let scratch = crate::platform::Fixture::new("revalidate-offline");
        let store = store(&scratch);
        let first = super::super::cache::ListingRequest::new(&source, false).unwrap();
        let api = listing_of("admin", "kept", "admin");
        revalidate(&source, &first, store.clone(), &api)
            .await
            .unwrap();
        let again = super::super::cache::ListingRequest::new(&source, false).unwrap();
        let offline = fixture(vec![Err((0, "offline".into()))]);
        assert_eq!(
            revalidate(&source, &again, store.clone(), &offline)
                .await
                .unwrap_err(),
            "offline"
        );
        let partial = fixture(vec![
            page(serde_json::json!({"login":"admin"}), false),
            Err((0, "offline".into())),
        ]);
        let list = revalidate(&source, &again, store.clone(), &partial)
            .await
            .unwrap();
        assert!(!list.errors.is_empty());
        let stored = again.read_stale(store).await.unwrap().unwrap();
        assert_eq!(stored.repos[0].name, "kept");
    }

    #[tokio::test]
    async fn authentication_failure_on_page_two_publishes_and_caches_nothing() {
        for status in [401, 403] {
            let source = revalidation_source(&format!("page-two-auth-{status}"));
            let scratch = crate::platform::Fixture::new(&format!("page-two-auth-{status}"));
            let store = store(&scratch);
            let request = super::super::cache::ListingRequest::new(&source, false).unwrap();
            let api = fixture(vec![
                page(serde_json::json!({"login":"admin"}), false),
                page(serde_json::json!([repo("admin", "first-page", true)]), true),
                Err((status, "denied".into())),
            ]);

            let published = revalidate(&source, &request, store.clone(), &api)
                .await
                .unwrap();

            assert!(published.repos.is_empty());
            assert_eq!(published.errors, ["admin: denied"]);
            assert!(!published.partial);
            assert!(request.read_stale(store.clone()).await.unwrap().is_none());
            store.close();
        }
    }

    #[test]
    fn enterprise_uses_api_v3_and_cloud_keeps_current_base() {
        assert_eq!(
            super::super::api_base("enterprise.invalid").unwrap(),
            "https://enterprise.invalid/api/v3"
        );
        assert_eq!(
            super::super::api_base("github.com").unwrap(),
            "https://api.github.com"
        );
    }
}
