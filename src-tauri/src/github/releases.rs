mod client;
mod connection;
mod repository;

pub use client::CreatedGithubRelease;

use super::pulls::PullsError;
use connection::{Connection, ReleaseTransport};
use tauri::AppHandle;

#[tauri::command]
pub async fn create_github_release(
    app: AppHandle,
    path: String,
    tag: String,
    notes: String,
    draft: Option<bool>,
    remote: Option<String>,
) -> Result<CreatedGithubRelease, PullsError> {
    create_with_connection(
        &app,
        ReleaseRequest {
            path: &path,
            tag: &tag,
            notes: &notes,
            draft: draft.unwrap_or(true),
            remote: remote.as_deref(),
        },
    )
    .await
}

struct ReleaseRequest<'a> {
    path: &'a str,
    tag: &'a str,
    notes: &'a str,
    draft: bool,
    remote: Option<&'a str>,
}

async fn create_with_connection(
    connection: &impl Connection,
    request: ReleaseRequest<'_>,
) -> Result<CreatedGithubRelease, PullsError> {
    client::validate_notes(request.notes)?;
    let tag = repository::annotated_object(request.path, request.tag).await?;
    let remotes = repository::remotes(request.path, request.remote).await?;
    let release = client::CreateRelease {
        tag: request.tag,
        commit: &tag.commit,
        notes: request.notes,
        draft: request.draft,
    };
    let mut failure = None;
    for remote in remotes {
        let resolved = async {
            let (repo, url) = connection.resolve(request.path, &remote).await?;
            let source = connection.load_source(request.path, &repo).await?;
            repository::require_published(request.path, &url, request.tag, &tag.object).await?;
            Ok::<_, PullsError>((repo, source))
        }
        .await;
        let (repo, source) = match resolved {
            Ok(resolved) => resolved,
            Err(error) => {
                failure.get_or_insert(error);
                continue;
            }
        };
        let http = connection.connect(&source, &repo.host).await?;
        if !http.authenticated() {
            return Err(
                "Store a GitHub token with repository contents write permission"
                    .to_string()
                    .into(),
            );
        }
        return Ok(client::create(&http, &repo, &release).await?);
    }
    Err(failure.unwrap_or_else(|| {
        "No configured GitHub remote matches this repository"
            .to_string()
            .into()
    }))
}

#[cfg(test)]
mod tests;
