use super::super::{http::Error, pulls::client::Transport, pulls::repository::Repository};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedGithubRelease {
    pub id: u64,
    pub url: String,
    pub draft: bool,
}

#[derive(Deserialize)]
struct GithubRelease {
    id: u64,
    html_url: String,
    draft: bool,
}

pub(super) fn validate_notes(notes: &str) -> Result<(), String> {
    if notes.len() > 64 * 1024 || notes.contains('\0') {
        return Err("Invalid release notes; use at most 64 KiB without null characters".into());
    }
    Ok(())
}

pub(super) struct CreateRelease<'a> {
    pub tag: &'a str,
    pub commit: &'a str,
    pub notes: &'a str,
    pub draft: bool,
}

pub(super) async fn create(
    transport: &impl Transport,
    repo: &Repository,
    request: &CreateRelease<'_>,
) -> Result<CreatedGithubRelease, Error> {
    let result = transport
        .send(
            Method::POST,
            &format!("{}/releases", repo.api_path()),
            Some(json!({"tag_name": request.tag, "target_commitish": request.commit, "body": request.notes, "draft": request.draft})),
        )
        .await?
        .decode::<GithubRelease>();
    let release = match result {
        Err(Error::Http { status: 404, .. }) => {
            return Err(Error::Message(
                "GitHub repository not found; check access and the remote".into(),
            ));
        }
        Err(Error::Http { status: 422, .. }) | Err(Error::AlreadyExists) => {
            return Err(Error::Message(
                "GitHub could not create the release; check the tag and any existing release"
                    .into(),
            ));
        }
        result => result?.data,
    };
    validate_release_url(&release.html_url, &repo.host)?;
    Ok(CreatedGithubRelease {
        id: release.id,
        url: release.html_url,
        draft: release.draft,
    })
}

fn validate_release_url(url: &str, host: &str) -> Result<(), Error> {
    let url = reqwest::Url::parse(url)
        .map_err(|_| Error::Message("Unexpected release URL from GitHub".into()))?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(host))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::Message("Unexpected release URL from GitHub".into()));
    }
    Ok(())
}
