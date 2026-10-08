use super::ci::{
    CiActionResult, CiArtifact, CiDispatch, CiDispatchForm, CiDownload, CiError, CiJob, CiLog,
    CiPage, CiQuery, CiRun,
};
use std::{future::Future, path::Path, pin::Pin};

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

    fn runs<'a>(
        &'a self,
        _query: &'a CiQuery,
    ) -> ProviderFuture<'a, Result<CiPage<CiRun>, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn jobs<'a>(
        &'a self,
        _run: &'a str,
        _query: &'a CiQuery,
    ) -> ProviderFuture<'a, Result<CiPage<CiJob>, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn log<'a>(&'a self, _job: &'a str) -> ProviderFuture<'a, Result<CiLog, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn artifacts<'a>(
        &'a self,
        _run: &'a str,
        _query: &'a CiQuery,
    ) -> ProviderFuture<'a, Result<CiPage<CiArtifact>, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn download_artifact<'a>(
        &'a self,
        _artifact: &'a str,
        _destination: &'a Path,
    ) -> ProviderFuture<'a, Result<CiDownload, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn download_logs<'a>(
        &'a self,
        _run: &'a str,
        _destination: &'a Path,
    ) -> ProviderFuture<'a, Result<CiDownload, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn dispatch_form<'a>(
        &'a self,
        _request: &'a CiDispatch,
    ) -> ProviderFuture<'a, Result<CiDispatchForm, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn rerun<'a>(
        &'a self,
        _run: &'a str,
        _failed_only: bool,
    ) -> ProviderFuture<'a, Result<CiActionResult, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
    fn cancel_run<'a>(
        &'a self,
        _run: &'a str,
    ) -> ProviderFuture<'a, Result<CiActionResult, Self::Error>>
    where
        Self::Error: From<CiError>,
    {
        Box::pin(async { Err(CiError::unsupported().into()) })
    }
}

pub trait IssueProvider: Send + Sync {
    type Issue;
    type Error;

    fn get<'a>(&'a self, id: &'a str) -> ProviderFuture<'a, Result<Self::Issue, Self::Error>>;
}
