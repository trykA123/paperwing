pub(crate) use super::http::Error;
use super::http::Http;
use super::pulls::repository::{parse_remote, Repository};
use super::{enc, valid_name};
use crate::settings::Source;
use serde::{Deserialize, Serialize};

pub(crate) mod blob_reference;
#[cfg(test)]
pub(crate) mod fixture;
mod mapping;
#[cfg(test)]
mod tests;

pub(crate) use mapping::{Change, Commit};

pub(crate) const FILE_LIMIT: usize = 300;
pub(crate) const COMMIT_LIMIT: usize = 250;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Truncated {
    pub files: bool,
    pub commits: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Comparison {
    pub base: String,
    pub head: String,
    pub merge_base: String,
    pub files: Vec<Change>,
    pub commits: Vec<Commit>,
    pub ahead: u64,
    pub behind: u64,
    pub truncated: Truncated,
}

#[derive(Clone)]
pub(crate) struct Request {
    pub source: Source,
    pub repository: Repository,
    pub base: String,
    pub head: String,
}

impl Request {
    pub(crate) fn from_url(source: Source, url: &str, refs: [String; 2]) -> Result<Self, Error> {
        let repository = parse_remote(url, &source.host)?;
        valid_name(&repository.owner)
            .and_then(|_| valid_name(&repository.name))
            .map_err(Error::Message)?;
        for reference in &refs {
            crate::git::valid_ref(reference).map_err(Error::Message)?;
            if reference.len() > 1024 || reference.contains(':') {
                return Err(Error::Message(
                    "Invalid same-repository comparison ref".into(),
                ));
            }
        }
        let [base, head] = refs;
        Ok(Self {
            source,
            repository,
            base,
            head,
        })
    }

    pub(in crate::github) async fn connect(&self) -> Result<Http<'_>, Error> {
        crate::providers::ensure_enabled(&self.source).map_err(Error::Message)?;
        let revision =
            crate::credentials::metadata_revision(&self.source).map_err(Error::Message)?;
        Http::connect_at(&self.source, revision)
            .await
            .map_err(Error::from)
    }
}

pub(crate) fn immutable(reference: &str) -> bool {
    matches!(reference.len(), 40 | 64) && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(in crate::github) async fn resolve(
    http: &Http<'_>,
    repository: &Repository,
    reference: &str,
) -> Result<String, Error> {
    if immutable(reference) {
        return Ok(reference.to_ascii_lowercase());
    }
    let path = format!("{}/commits/{}", repository.api_path(), enc(reference));
    let response: mapping::Sha = http.compare_get(&path).await?.data;
    mapping::valid_sha(response.sha)
}

pub(in crate::github) async fn fetch(
    http: &Http<'_>,
    request: &Request,
) -> Result<Comparison, Error> {
    let base = resolve(http, &request.repository, &request.base).await?;
    let head = resolve(http, &request.repository, &request.head).await?;
    fetch_resolved(http, &request.repository, base, head).await
}

pub(in crate::github) async fn fetch_resolved(
    http: &Http<'_>,
    repository: &Repository,
    base: String,
    head: String,
) -> Result<Comparison, Error> {
    let mut result = None;
    for page in 1..=3 {
        let path = format!(
            "{}/compare/{base}...{head}?per_page=100&page={page}",
            repository.api_path()
        );
        let response = http.compare_get::<mapping::Page>(&path).await?;
        let data = response.data;
        mapping::validate_page(&data, &base)?;
        let comparison =
            result.get_or_insert_with(|| mapping::comparison(&data, [base.clone(), head.clone()]));
        if comparison.merge_base != data.merge_base_commit.sha
            || comparison.ahead != data.ahead_by
            || comparison.behind != data.behind_by
        {
            return Err(Error::Message(
                "GitHub comparison changed during pagination".into(),
            ));
        }
        comparison.commits.extend(
            data.commits
                .into_iter()
                .take(COMMIT_LIMIT - comparison.commits.len()),
        );
        if comparison.commits.len() >= COMMIT_LIMIT {
            comparison.truncated.commits = true;
            break;
        }
        if !response.next {
            break;
        }
        if page == 3 {
            comparison.truncated.commits = true;
        }
    }
    result.ok_or_else(|| Error::Message("GitHub comparison unavailable".into()))
}

pub(crate) async fn load(request: &Request) -> Result<Comparison, Error> {
    let http = request.connect().await?;
    fetch(&http, request).await
}

#[cfg(test)]
pub(crate) use super::http::fixture::Binding;
