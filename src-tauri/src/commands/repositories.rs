use crate::github;

domain! {
            github::test_source,
            github::list_user_orgs,
            github::list_repos,
            github::list_cached_repos,
            github::get_commits,
            github::pulls::pull_for_branch,
            github::pulls::open_pull_request,
            github::releases::create_github_release,
}
