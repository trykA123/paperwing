use super::super::{
    http::{GithubApi, Http},
    pulls::{self, client::Transport, repository::Repository, source, PullsError},
};
use super::repository;
use crate::settings::Source;
use tauri::AppHandle;

pub(super) trait ReleaseTransport: Transport {
    fn authenticated(&self) -> bool;
}

impl ReleaseTransport for Http<'_> {
    fn authenticated(&self) -> bool {
        GithubApi::authenticated(self)
    }
}

pub(super) trait Connection {
    type Transport<'a>: ReleaseTransport
    where
        Self: 'a;

    async fn resolve(&self, path: &str, remote: &str) -> Result<(Repository, String), PullsError> {
        repository::resolve(path, remote).await
    }

    async fn load_source(&self, path: &str, repo: &Repository) -> Result<Source, PullsError>;

    async fn connect<'a>(
        &'a self,
        source: &'a Source,
        host: &'a str,
    ) -> Result<Self::Transport<'a>, PullsError>;
}

impl Connection for AppHandle {
    type Transport<'a> = Http<'a>;

    async fn load_source(&self, path: &str, repo: &Repository) -> Result<Source, PullsError> {
        Ok(source::load(self.clone(), path.to_string(), repo).await?)
    }

    async fn connect<'a>(
        &'a self,
        source: &'a Source,
        host: &'a str,
    ) -> Result<Self::Transport<'a>, PullsError> {
        pulls::connect(source, host).await
    }
}
