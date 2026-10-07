use super::{cache, http, listing, pulls, Commit, RepoList};
use crate::kernel::{
    capabilities::{ProviderFuture, PullRequestProvider, RepositoryProvider, RepositoryRevision},
    registry::ProviderKey,
};
use crate::{settings::Source, store::Store};

pub(crate) struct GithubProvider {
    key: ProviderKey,
    store: Option<Store>,
}

impl GithubProvider {
    pub(crate) fn new(key: ProviderKey, store: Option<Store>) -> Self {
        Self { key, store }
    }

    fn store(&self) -> Result<Store, String> {
        self.store
            .clone()
            .ok_or("Repository store is unavailable".into())
    }

    #[cfg(test)]
    pub(super) async fn list_with(
        &self,
        source: &Source,
        refresh: bool,
        api: &impl http::GithubApi,
    ) -> Result<RepoList, String> {
        let request = cache::ListingRequest::new(source, refresh)?;
        listing::revalidate(source, &request, self.store()?, api).await
    }
}

impl RepositoryProvider for GithubProvider {
    type Source = Source;
    type Listing = RepoList;
    type Commit = Commit;
    type Refs = crate::git::RefsResult;
    type Error = String;

    fn list<'a>(
        &'a self,
        source: &'a Source,
        refresh: bool,
    ) -> ProviderFuture<'a, Result<RepoList, String>> {
        Box::pin(async move {
            if source.kind == "manual" {
                return Ok(RepoList {
                    repos: source
                        .urls
                        .iter()
                        .filter_map(|url| super::parse_manual(source, url))
                        .collect(),
                    fetched_at: cache::now(),
                    ..Default::default()
                });
            }
            let request = cache::ListingRequest::new(source, refresh)?;
            let http = http::Http::connect_at(source, request.revision())
                .await
                .map_err(|(_, reason)| reason)?;
            listing::revalidate(source, &request, self.store()?, &http).await
        })
    }

    fn cached<'a>(
        &'a self,
        source: &'a Source,
    ) -> ProviderFuture<'a, Result<Option<RepoList>, String>> {
        Box::pin(async move {
            if source.kind == "manual" {
                return Ok(None);
            }
            cache::ListingRequest::new(source, false)?
                .read_stale(self.store()?)
                .await
        })
    }

    fn commits<'a>(
        &'a self,
        source: &'a Source,
        revision: RepositoryRevision<'a>,
    ) -> ProviderFuture<'a, Result<Vec<Commit>, String>> {
        Box::pin(super::fetch_commits(
            source,
            revision.owner,
            revision.name,
            revision.branch,
        ))
    }

    fn refs(&self, urls: Vec<String>) -> ProviderFuture<'_, Vec<crate::git::RefsResult>> {
        Box::pin(crate::git::remote_refs::get_refs_many(urls))
    }
}

impl PullRequestProvider for GithubProvider {
    type Source = Source;
    type Branch = pulls::repository::Branch;
    type PullRequest = pulls::PullRequest;
    type OpenRequest = pulls::OpenPullRequest;
    type Created = pulls::CreatedPullRequest;
    type Error = pulls::PullsError;

    fn pull_for_branch<'a>(
        &'a self,
        source: &'a Source,
        branch: &'a Self::Branch,
    ) -> ProviderFuture<'a, Result<Option<Self::PullRequest>, Self::Error>> {
        Box::pin(async move {
            let http = pulls::connect(source, &self.key.host).await?;
            Ok(pulls::client::pull_for_branch(&http, branch).await?)
        })
    }

    fn open<'a>(
        &'a self,
        source: &'a Source,
        branch: &'a Self::Branch,
        request: &'a Self::OpenRequest,
    ) -> ProviderFuture<'a, Result<Self::Created, Self::Error>> {
        Box::pin(async move {
            let http = pulls::connect(source, &self.key.host).await?;
            Ok(pulls::client::open_pull_request(&http, branch, request).await?)
        })
    }
}

#[cfg(test)]
mod tests;
