pub(super) mod client;
mod error;
mod model;
pub(super) mod repository;
pub(super) mod source;
mod summary;

pub use error::PullsError;
pub use model::{CreatedPullRequest, OpenPullRequest, PullRequest};

use super::http::Http;
use tauri::AppHandle;

pub(super) async fn connect<'a>(
    source: &'a crate::settings::Source,
    host: &'a str,
) -> Result<Http<'a>, PullsError> {
    let revision = crate::credentials::metadata_revision(source)?;
    if source.kind == "manual" {
        return Ok(Http::connect_host_at(source, revision, host).await?);
    }
    Ok(Http::connect_at(source, revision).await?)
}

#[tauri::command]
pub async fn pull_for_branch(
    app: AppHandle,
    path: String,
    branch: String,
) -> Result<Option<PullRequest>, PullsError> {
    repository::validate(&path, &branch).await?;
    let branch = repository::resolve(&path, &branch).await?;
    let source = source::load(app, path, &branch.repo).await?;
    let http = connect(&source, &branch.repo.host).await?;
    Ok(client::pull_for_branch(&http, &branch).await?)
}

#[tauri::command]
pub async fn open_pull_request(
    app: AppHandle,
    path: String,
    request: OpenPullRequest,
) -> Result<CreatedPullRequest, PullsError> {
    request.validate()?;
    repository::validate(&path, &request.head).await?;
    let branch = repository::resolve(&path, &request.head).await?;
    let source = source::load(app, path, &branch.repo).await?;
    let http = connect(&source, &branch.repo.host).await?;
    Ok(client::open_pull_request(&http, &branch, &request).await?)
}

#[cfg(test)]
mod tests;
