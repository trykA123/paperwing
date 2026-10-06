use crate::settings::{valid_id, Source};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

mod cache;
mod commit_cache;
mod http;
mod listing;
use http::{get_json, GithubApi};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub id: String,
    pub source: String,
    pub org: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub default_branch: String,
    pub pushed_at: String,
    pub archived: bool,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepoList {
    pub repos: Vec<Repo>,
    pub fetched_at: u64,
    pub errors: Vec<String>,
    #[serde(default)]
    pub stale: bool,
}

pub use crate::store::CommitRow as Commit;

#[derive(Deserialize)]
struct GhOwner {
    login: String,
}
#[derive(Deserialize)]
struct GhRepo {
    name: String,
    owner: GhOwner,
    description: Option<String>,
    ssh_url: String,
    default_branch: Option<String>,
    pushed_at: Option<String>,
    #[serde(default)]
    archived: bool,
}
#[derive(Deserialize)]
struct GhLogin {
    login: String,
}
#[derive(Deserialize)]
struct GhCommit {
    sha: String,
    commit: GhCommitInner,
    #[serde(default)]
    parents: Vec<GhSha>,
}
#[derive(Deserialize)]
struct GhSha {
    sha: String,
}
#[derive(Deserialize)]
struct GhCommitInner {
    message: String,
    author: Option<GhAuthor>,
}
#[derive(Deserialize)]
struct GhAuthor {
    name: Option<String>,
    date: Option<String>,
}

fn valid_host(host: &str) -> Result<(), String> {
    let ok = !host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['.', '-'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':');
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid host: {host}"))
    }
}

fn valid_name(s: &str) -> Result<(), String> {
    let ok = !s.is_empty()
        && s.len() <= 100
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid name: {s}"))
    }
}

fn api_base(host: &str) -> Result<String, String> {
    valid_host(host)?;
    #[cfg(feature = "test-profile")]
    if let Some(endpoint) = crate::test_profile::github_endpoint()? {
        return Ok(endpoint);
    }
    Ok(if host == "github.com" {
        "https://api.github.com".into()
    } else {
        format!("https://{host}/api/v3")
    })
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn parse_manual(source: &Source, url: &str) -> Option<Repo> {
    let u = url.trim();
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let path = if let Some(rest) = u
        .strip_prefix("ssh://")
        .or_else(|| u.strip_prefix("https://"))
        .or_else(|| u.strip_prefix("http://"))
    {
        rest.split_once('/')?.1
    } else {
        u.split_once(':')?.1
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let (org, name) = path.rsplit_once('/')?;
    if org.is_empty() || name.is_empty() {
        return None;
    }
    Some(Repo {
        id: format!("{}:{}/{}", source.id, org, name),
        source: source.id.clone(),
        org: org.into(),
        name: name.into(),
        description: String::new(),
        url: u.into(),
        default_branch: String::new(),
        pushed_at: String::new(),
        archived: false,
    })
}

#[tauri::command]
pub async fn list_repos(app: AppHandle, source: Source, refresh: bool) -> Result<RepoList, String> {
    valid_id(&source.id)?;
    let fetched_at = cache::now();
    if source.kind == "manual" {
        let repos = source
            .urls
            .iter()
            .filter_map(|u| parse_manual(&source, u))
            .collect();
        return Ok(RepoList {
            repos,
            fetched_at,
            ..Default::default()
        });
    }
    let request = cache::ListingRequest::new(&source, refresh)?;
    let http = http::Http::connect_at(&source, request.revision())
        .await
        .map_err(|(_, reason)| reason)?;
    listing::revalidate(
        &source,
        &request,
        app.state::<Store>().inner().clone(),
        &http,
    )
    .await
}

#[tauri::command]
pub async fn list_cached_repos(app: AppHandle, source: Source) -> Result<Option<RepoList>, String> {
    valid_id(&source.id)?;
    if source.kind == "manual" {
        return Ok(None);
    }
    cache::ListingRequest::new(&source, false)?
        .read_stale(app.state::<Store>().inner().clone())
        .await
}

#[tauri::command]
pub async fn test_source(source: Source) -> Result<String, String> {
    valid_id(&source.id)?;
    let login = get_json::<GhLogin>(&source, "/user")
        .await
        .map_err(|(_, e)| e)?
        .login;
    valid_name(&login)?;
    Ok(login)
}

#[tauri::command]
pub async fn list_user_orgs(source: Source) -> Result<Vec<String>, String> {
    valid_id(&source.id)?;
    let orgs: Vec<GhLogin> = get_json(&source, "/user/orgs?per_page=100")
        .await
        .map_err(|(_, e)| e)?;
    Ok(orgs.into_iter().map(|o| o.login).collect())
}

#[tauri::command]
pub async fn get_commits(
    app: AppHandle,
    source: Source,
    org: String,
    name: String,
    branch: String,
) -> Result<Vec<Commit>, String> {
    valid_id(&source.id)?;
    valid_name(&org)?;
    valid_name(&name)?;
    let mut path = format!("/repos/{org}/{name}/commits?per_page=60");
    if !branch.is_empty() {
        path.push_str(&format!("&sha={}", enc(&branch)));
    }
    let revision = crate::credentials::metadata_revision(&source)?;
    let list: Vec<GhCommit> = http::Http::connect_at(&source, revision)
        .await
        .map_err(|(_, reason)| reason)?
        .get(&path)
        .await
        .map_err(|(_, reason)| reason)?
        .data;
    let commits: Vec<Commit> = list
        .into_iter()
        .map(|c| {
            let (author, date) = c
                .commit
                .author
                .map(|a| (a.name.unwrap_or_default(), a.date.unwrap_or_default()))
                .unwrap_or_default();
            Commit {
                sha: c.sha,
                message: c.commit.message,
                author,
                date,
                parents: c.parents.into_iter().map(|p| p.sha).collect(),
            }
        })
        .collect();
    commit_cache::remember(
        &app.state::<Store>(),
        &commit_cache::Request {
            source: &source,
            org: &org,
            name: &name,
            branch: &branch,
            revision,
        },
        &commits,
    );
    Ok(commits)
}
