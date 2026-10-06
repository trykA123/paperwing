use super::super::{
    enc,
    http::{Error, Http, Response},
};
use super::model::{
    CommitStatus, CreatedPullRequest, GithubPull, OpenPullRequest, PullRequest, Review,
};
use super::repository::Repository;
use super::summary;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;

const PAGE_LIMIT: usize = 20;

pub(super) trait Transport {
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

pub(super) async fn pull_for_branch(
    transport: &impl Transport,
    repo: &Repository,
    branch: &str,
) -> Result<Option<PullRequest>, Error> {
    let pull = match find_pull(transport, &repo.pulls_path(branch, "open")).await? {
        Some(pull) => pull,
        None => match find_closed_pull(transport, &repo.pulls_path(branch, "closed")).await? {
            Some(pull) => pull,
            None => return Ok(None),
        },
    };
    let state = pull.state()?;
    let root = repo.api_path();
    let reviews: Vec<Review> = collect(
        transport,
        &format!("{root}/pulls/{}/reviews", pull.number),
        None,
    )
    .await?;
    let commit = format!("{root}/commits/{}", enc(&pull.head.sha));
    let runs = collect(
        transport,
        &format!("{commit}/check-runs?filter=latest"),
        Some("check_runs"),
    )
    .await?;
    let statuses: Vec<CommitStatus> =
        collect(transport, &format!("{commit}/statuses"), None).await?;
    Ok(Some(PullRequest {
        number: pull.number,
        title: pull.title,
        url: pull.html_url,
        state,
        base: pull.base.name,
        head_sha: pull.head.sha,
        review_state: summary::review_state(&reviews, state),
        checks: summary::checks(runs, statuses),
    }))
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

pub(super) struct Creation<'a> {
    pub request: &'a OpenPullRequest,
    pub published: bool,
}

pub(super) async fn open_pull_request(
    transport: &impl Transport,
    repo: &Repository,
    creation: Creation<'_>,
) -> Result<CreatedPullRequest, Error> {
    let request = creation.request;
    request.validate()?;
    require_published(creation.published)?;
    let body = serde_json::to_value(request)
        .map_err(|_| Error::Message("Cannot encode pull request".into()))?;
    let result = transport
        .send(
            Method::POST,
            &format!("{}/pulls", repo.api_path()),
            Some(body),
        )
        .await?
        .decode();
    match result {
        Err(Error::AlreadyExists) => {
            let pull = find_pull(transport, &repo.pulls_path(&request.head, "open")).await?
                .ok_or_else(|| Error::Message("A pull request already exists, but GitHub did not return it; refresh and try again".into()))?;
            Ok(CreatedPullRequest {
                number: pull.number,
                url: pull.html_url,
            })
        }
        result => result.map(|page| page.data),
    }
}
