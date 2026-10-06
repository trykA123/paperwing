mod client;
mod model;
mod repository;
mod source;
mod summary;

pub use model::{CreatedPullRequest, OpenPullRequest, PullRequest};

use super::http::Http;
use tauri::AppHandle;

#[tauri::command]
pub async fn pull_for_branch(
    app: AppHandle,
    path: String,
    branch: String,
) -> Result<Option<PullRequest>, String> {
    repository::validate(&path, &branch).await?;
    let repo = repository::resolve(&path, &branch)
        .await
        .map_err(|error| error.to_string())?;
    let source = source::load(app, path, &repo).await?;
    let revision = crate::credentials::metadata_revision(&source)?;
    let http = Http::connect_github_at(&source, revision)
        .await
        .map_err(|(_, message)| message)?;
    client::pull_for_branch(&http, &repo, &branch)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn open_pull_request(
    app: AppHandle,
    path: String,
    request: OpenPullRequest,
) -> Result<CreatedPullRequest, String> {
    request.validate().map_err(|error| error.to_string())?;
    repository::validate(&path, &request.head).await?;
    let repo = repository::resolve(&path, &request.head)
        .await
        .map_err(|error| error.to_string())?;
    let published = repository::is_published(&path, &repo, &request.head)
        .await
        .map_err(|error| error.to_string())?;
    client::require_published(published).map_err(|error| error.to_string())?;
    let source = source::load(app, path, &repo).await?;
    let revision = crate::credentials::metadata_revision(&source)?;
    let http = Http::connect_github_at(&source, revision)
        .await
        .map_err(|(_, message)| message)?;
    client::open_pull_request(
        &http,
        &repo,
        client::Creation {
            request: &request,
            published,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
