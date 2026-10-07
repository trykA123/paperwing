use super::super::{
    enc,
    http::{Error, Http, Response},
};
use super::model::{
    CommitStatus, CreatedPullRequest, GithubCreatedPull, GithubPull, GithubRepository,
    OpenPullRequest, PublishedBranch, PullRequest, Review,
};
use super::repository::{Branch, Repository};
use super::summary;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;

const PAGE_LIMIT: usize = 20;

pub(in crate::github) trait Transport {
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Response, Error>;
}

impl Transport for Http<'_> {
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Response, Error> {
        self.send(method, path, body).await
    }
}

pub(in crate::github) async fn pull_for_branch(
    transport: &impl Transport,
    branch: &Branch,
) -> Result<Option<PullRequest>, Error> {
    let repo = target_repository(transport, &branch.repo).await?;
    let head_owner = &branch.repo.owner;
    let head = &branch.head;
    let pull = match find_pull(transport, &repo.pulls_path(head_owner, head, "open")).await? {
        Some(pull) => pull,
        None => {
            match find_closed_pull(transport, &repo.pulls_path(head_owner, head, "closed")).await? {
                Some(pull) if pull.head.sha == branch.sha => pull,
                _ => return Ok(None),
            }
        }
    };
    let state = pull.state()?;
    let root = repo.api_path();
    let reviews: Vec<Review> = collect(
        transport,
        &format!("{root}/pulls/{}/reviews", pull.number),
        None,
    )
    .await?;
    let checks = load_checks(transport, &root, &pull.head.sha).await?;
    Ok(Some(PullRequest {
        number: pull.number,
        title: pull.title,
        url: pull.html_url,
        state,
        base: pull.base.name,
        has_unpushed_commits: pull.head.sha != branch.sha,
        head_sha: pull.head.sha,
        target_repo: format!("{}/{}", repo.owner, repo.name),
        review_state: summary::review_state(&reviews, state),
        checks,
    }))
}

async fn load_checks(
    transport: &impl Transport,
    root: &str,
    sha: &str,
) -> Result<super::model::Checks, Error> {
    let commit = format!("{root}/commits/{}", enc(sha));
    let runs = collect(
        transport,
        &format!("{commit}/check-runs?filter=latest"),
        Some("check_runs"),
    )
    .await?;
    let statuses: Vec<CommitStatus> =
        collect(transport, &format!("{commit}/status"), Some("statuses")).await?;
    Ok(summary::checks(runs, statuses))
}

async fn find_pull(transport: &impl Transport, path: &str) -> Result<Option<GithubPull>, Error> {
    let path = format!("{path}&per_page=1&page=1");
    let pulls: Vec<GithubPull> = transport
        .send(Method::GET, &path, None)
        .await?
        .decode()?
        .data;
    Ok(pulls.into_iter().next())
}

async fn find_closed_pull(
    transport: &impl Transport,
    path: &str,
) -> Result<Option<GithubPull>, Error> {
    let pulls: Vec<GithubPull> = collect(transport, path, None).await?;
    Ok(pulls
        .into_iter()
        .max_by_key(|pull| (pull.closed_at.clone(), pull.number)))
}

async fn collect<T: DeserializeOwned>(
    transport: &impl Transport,
    path: &str,
    field: Option<&str>,
) -> Result<Vec<T>, Error> {
    let mut result = Vec::new();
    for page in 1..=PAGE_LIMIT {
        let separator = if path.contains('?') { '&' } else { '?' };
        let path = format!("{path}{separator}per_page=100&page={page}");
        let response = transport
            .send(Method::GET, &path, None)
            .await?
            .decode::<Value>()?;
        let data = match field {
            Some(field) => response
                .data
                .get(field)
                .cloned()
                .ok_or_else(|| Error::Message("Unexpected response from GitHub".into()))?,
            None => response.data,
        };
        let values: Vec<T> = serde_json::from_value(data)
            .map_err(|_| Error::Message("Unexpected response from GitHub".into()))?;
        result.extend(values);
        if !response.next {
            return Ok(result);
        }
    }
    Err(Error::Message(
        "GitHub pagination limit reached; too many pull requests, reviews or checks to summarize safely".into(),
    ))
}

pub(super) fn require_published(published: bool) -> Result<(), Error> {
    if !published {
        return Err(Error::Message(
            "Branch is not on the remote; push it before opening a pull request".into(),
        ));
    }
    Ok(())
}

async fn target_repository(
    transport: &impl Transport,
    repo: &Repository,
) -> Result<Repository, Error> {
    let metadata: GithubRepository = transport
        .send(Method::GET, &repo.api_path(), None)
        .await?
        .decode()?
        .data;
    if !metadata.fork {
        return Ok(repo.clone());
    }
    let parent = metadata
        .parent
        .ok_or_else(|| Error::Message("GitHub did not return the fork parent repository".into()))?;
    let (owner, name) = parent
        .full_name
        .split_once('/')
        .ok_or_else(|| Error::Message("Unexpected fork parent repository from GitHub".into()))?;
    super::super::valid_name(owner)
        .and_then(|_| super::super::valid_name(name))
        .map_err(|_| Error::Message("Unexpected fork parent repository from GitHub".into()))?;
    Ok(Repository {
        owner: owner.into(),
        name: name.into(),
        host: repo.host.clone(),
    })
}

async fn has_unpushed_commits(transport: &impl Transport, branch: &Branch) -> Result<bool, Error> {
    let path = format!("{}/branches/{}", branch.repo.api_path(), enc(&branch.head));
    let response = transport.send(Method::GET, &path, None).await?;
    require_published(response.status != 404)?;
    let status = response.status;
    let remote: PublishedBranch = response.decode()?.data;
    if status != 200 {
        return Err(Error::Message(
            "Unexpected branch response from GitHub".into(),
        ));
    }
    Ok(remote.commit.sha != branch.sha)
}

pub(in crate::github) async fn open_pull_request(
    transport: &impl Transport,
    branch: &Branch,
    request: &OpenPullRequest,
) -> Result<CreatedPullRequest, Error> {
    request.validate()?;
    let has_unpushed_commits = has_unpushed_commits(transport, branch).await?;
    let repo = target_repository(transport, &branch.repo).await?;
    if repo.same_repo(&branch.repo) && branch.head == request.base {
        return Err(Error::Message(
            "Choose a base branch different from the head branch".into(),
        ));
    }
    let mut body = serde_json::to_value(request)
        .map_err(|_| Error::Message("Cannot encode pull request".into()))?;
    body["head"] = if repo.same_repo(&branch.repo) {
        branch.head.clone().into()
    } else {
        format!("{}:{}", branch.repo.owner, branch.head).into()
    };
    let result = transport
        .send(
            Method::POST,
            &format!("{}/pulls", repo.api_path()),
            Some(body),
        )
        .await?
        .decode::<GithubCreatedPull>();
    let pull = match result {
        Err(Error::AlreadyExists) => existing_pull(transport, &repo, branch).await?,
        result => result?.data,
    };
    Ok(CreatedPullRequest {
        number: pull.number,
        url: pull.html_url,
        target_repo: format!("{}/{}", repo.owner, repo.name),
        has_unpushed_commits,
    })
}

async fn existing_pull(
    transport: &impl Transport,
    repo: &Repository,
    branch: &Branch,
) -> Result<GithubCreatedPull, Error> {
    let pull = find_pull(
        transport,
        &repo.pulls_path(&branch.repo.owner, &branch.head, "open"),
    )
    .await?
    .ok_or_else(|| {
        Error::Message(
            "A pull request already exists, but GitHub did not return it; refresh and try again"
                .into(),
        )
    })?;
    Ok(GithubCreatedPull {
        number: pull.number,
        html_url: pull.html_url,
    })
}
