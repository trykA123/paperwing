use crate::settings::{get_token, valid_id, Source};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoList {
    pub repos: Vec<Repo>,
    pub fetched_at: u64,
    pub errors: Vec<String>,
}

#[derive(Serialize)]
pub struct Commit {
    sha: String,
    message: String,
    author: String,
    date: String,
    parents: Vec<String>,
}

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
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':');
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
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid name: {s}"))
    }
}

fn api_base(host: &str) -> Result<String, String> {
    valid_host(host)?;
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

async fn get_json<T: DeserializeOwned>(source: &Source, path: &str) -> Result<T, (u16, String)> {
    let base = api_base(&source.host).map_err(|e| (0, e))?;
    let client = reqwest::Client::builder()
        .user_agent("paperwing")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| (0, e.to_string()))?;
    let mut req = client
        .get(format!("{base}{path}"))
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = get_token(&source.id) {
        req = req.bearer_auth(token);
    }
    let res = req
        .send()
        .await
        .map_err(|e| (0, format!("Cannot reach {}: {e}", source.host)))?;
    let status = res.status().as_u16();
    if !res.status().is_success() {
        let msg = match status {
            401 => "Token is missing or invalid".to_string(),
            403 => "Access denied or rate limited (token needs the repo and read:org scopes)".into(),
            404 => "Not found".into(),
            s => format!("HTTP {s}"),
        };
        return Err((status, msg));
    }
    res.json::<T>()
        .await
        .map_err(|e| (0, format!("Unexpected response from {}: {e}", source.host)))
}

fn to_repo(source: &Source, r: GhRepo) -> Repo {
    let org = r.owner.login;
    Repo {
        id: format!("{}:{}/{}", source.id, org, r.name),
        source: source.id.clone(),
        org,
        name: r.name,
        description: r.description.unwrap_or_default(),
        url: r.ssh_url,
        default_branch: r.default_branch.unwrap_or_default(),
        pushed_at: r.pushed_at.unwrap_or_default(),
        archived: r.archived,
    }
}

async fn list_owner(source: &Source, owner: &str) -> Result<Vec<Repo>, String> {
    valid_name(owner)?;
    let mut out = Vec::new();
    let mut is_org = true;
    let mut page = 1;
    loop {
        let path = if is_org {
            format!("/orgs/{owner}/repos?type=all&per_page=100&page={page}")
        } else {
            format!("/users/{owner}/repos?type=owner&per_page=100&page={page}")
        };
        match get_json::<Vec<GhRepo>>(source, &path).await {
            Ok(batch) => {
                let n = batch.len();
                out.extend(batch.into_iter().map(|r| to_repo(source, r)));
                if n < 100 || page >= 100 {
                    break;
                }
                page += 1;
            }
            // Not an organization: retry as a user account.
            Err((404, _)) if is_org && page == 1 => is_org = false,
            Err((_, e)) => return Err(format!("{owner}: {e}")),
        }
    }
    Ok(out)
}

fn parse_manual(source: &Source, url: &str) -> Option<Repo> {
    let u = url.trim();
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
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

fn cache_file(app: &AppHandle, source_id: &str) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("repos-{source_id}.json")))
}

#[tauri::command]
pub async fn list_repos(app: AppHandle, source: Source, refresh: bool) -> Result<RepoList, String> {
    valid_id(&source.id)?;
    let fetched_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if source.kind == "manual" {
        let repos = source.urls.iter().filter_map(|u| parse_manual(&source, u)).collect();
        return Ok(RepoList { repos, fetched_at, errors: vec![] });
    }
    let cache = cache_file(&app, &source.id)?;
    if !refresh {
        if let Some(list) = std::fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<RepoList>(&t).ok()) {
            return Ok(list);
        }
    }
    let mut list = RepoList { fetched_at, ..Default::default() };
    for owner in &source.orgs {
        match list_owner(&source, owner).await {
            Ok(repos) => list.repos.extend(repos),
            Err(e) => list.errors.push(e),
        }
    }
    if list.errors.is_empty() {
        let _ = std::fs::write(&cache, serde_json::to_string(&list).unwrap_or_default());
    }
    Ok(list)
}

#[tauri::command]
pub async fn test_source(source: Source) -> Result<String, String> {
    valid_id(&source.id)?;
    get_json::<GhLogin>(&source, "/user").await.map(|u| u.login).map_err(|(_, e)| e)
}

#[tauri::command]
pub async fn list_user_orgs(source: Source) -> Result<Vec<String>, String> {
    valid_id(&source.id)?;
    let orgs: Vec<GhLogin> = get_json(&source, "/user/orgs?per_page=100").await.map_err(|(_, e)| e)?;
    Ok(orgs.into_iter().map(|o| o.login).collect())
}

#[tauri::command]
pub async fn get_commits(source: Source, org: String, name: String, branch: String) -> Result<Vec<Commit>, String> {
    valid_id(&source.id)?;
    valid_name(&org)?;
    valid_name(&name)?;
    let mut path = format!("/repos/{org}/{name}/commits?per_page=60");
    if !branch.is_empty() {
        path.push_str(&format!("&sha={}", enc(&branch)));
    }
    let list: Vec<GhCommit> = get_json(&source, &path).await.map_err(|(_, e)| e)?;
    Ok(list
        .into_iter()
        .map(|c| {
            let (author, date) = c
                .commit
                .author
                .map(|a| (a.name.unwrap_or_default(), a.date.unwrap_or_default()))
                .unwrap_or_default();
            Commit { sha: c.sha, message: c.commit.message, author, date, parents: c.parents.into_iter().map(|p| p.sha).collect() }
        })
        .collect())
}
