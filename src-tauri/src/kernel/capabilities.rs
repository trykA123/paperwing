use std::{future::Future, pin::Pin};

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct RepositoryRevision<'a> {
    pub owner: &'a str,
    pub name: &'a str,
    pub branch: &'a str,
}

pub trait RepositoryProvider: Send + Sync {
    type Source;
    type Listing;
    type Commit;
    type Refs;
    type Error;

    fn list<'a>(
        &'a self,
        source: &'a Self::Source,
        refresh: bool,
    ) -> ProviderFuture<'a, Result<Self::Listing, Self::Error>>;
    fn cached<'a>(
        &'a self,
        source: &'a Self::Source,
    ) -> ProviderFuture<'a, Result<Option<Self::Listing>, Self::Error>>;
    fn commits<'a>(
        &'a self,
        source: &'a Self::Source,
        revision: RepositoryRevision<'a>,
    ) -> ProviderFuture<'a, Result<Vec<Self::Commit>, Self::Error>>;
    fn refs(&self, urls: Vec<String>) -> ProviderFuture<'_, Vec<Self::Refs>>;
}

pub trait PullRequestProvider: Send + Sync {
    type Source;
    type Branch;
    type PullRequest;
    type OpenRequest;
    type Created;
    type Error;

    fn pull_for_branch<'a>(
        &'a self,
        source: &'a Self::Source,
        branch: &'a Self::Branch,
    ) -> ProviderFuture<'a, Result<Option<Self::PullRequest>, Self::Error>>;
    fn open<'a>(
        &'a self,
        source: &'a Self::Source,
        branch: &'a Self::Branch,
        request: &'a Self::OpenRequest,
    ) -> ProviderFuture<'a, Result<Self::Created, Self::Error>>;
}

pub trait CiProvider: Send + Sync {
    type Request;
    type Run;
    type Error;

    fn start(&self, request: Self::Request) -> ProviderFuture<'_, Result<Self::Run, Self::Error>>;
    fn cancel<'a>(&'a self, run: &'a Self::Run) -> ProviderFuture<'a, Result<(), Self::Error>>;
}

pub trait IssueProvider: Send + Sync {
    type Issue;
    type Error;

    fn get<'a>(&'a self, id: &'a str) -> ProviderFuture<'a, Result<Self::Issue, Self::Error>>;
}
