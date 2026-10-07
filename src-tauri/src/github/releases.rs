mod client;
mod repository;

pub use client::CreatedGithubRelease;

use super::http::GithubApi;
use super::pulls::{connect, source, PullsError};
use tauri::AppHandle;

#[tauri::command]
pub async fn create_github_release(
    app: AppHandle,
    path: String,
    tag: String,
    notes: String,
    draft: Option<bool>,
) -> Result<CreatedGithubRelease, PullsError> {
    client::validate_notes(&notes)?;
    let object = repository::annotated_object(&path, &tag).await?;
    let remotes = repository::remotes(&path).await?;
    let mut failure = "No configured GitHub remote matches this repository".to_string();
    for remote in remotes {
        let resolved = async {
            let (repo, url) = repository::resolve(&path, &remote).await?;
            let source = source::load(app.clone(), path.clone(), &repo).await?;
            repository::require_published(&path, &url, &tag, &object).await?;
            Ok::<_, PullsError>((repo, source))
        }
        .await;
        let (repo, source) = match resolved {
            Ok(resolved) => resolved,
            Err(error) => {
                failure = error.to_string();
                continue;
            }
        };
        let http = connect(&source, &repo.host).await?;
        if !http.authenticated() {
            return Err(
                "Store a GitHub token with repository contents write permission"
                    .to_string()
                    .into(),
            );
        }
        return Ok(client::create(&http, &repo, &tag, &notes, draft.unwrap_or(true)).await?);
    }
    Err(failure.into())
}

#[cfg(test)]
mod tests;
