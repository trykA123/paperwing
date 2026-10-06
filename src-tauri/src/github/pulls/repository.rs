use super::super::{enc, http::Error, parse_manual_path, valid_name};
use crate::git::{execute, Captured, OutputPolicy, Request};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Repository {
    pub owner: String,
    pub name: String,
    pub remote: String,
}

impl Repository {
    pub fn api_path(&self) -> String {
        format!("/repos/{}/{}", enc(&self.owner), enc(&self.name))
    }

    pub fn pulls_path(&self, branch: &str, state: &str) -> String {
        let head = enc(&format!("{}:{branch}", self.owner));
        format!(
            "{}/pulls?head={head}&state={state}&sort=updated&direction=desc",
            self.api_path()
        )
    }

    pub fn same_repo(&self, other: &Self) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner) && self.name.eq_ignore_ascii_case(&other.name)
    }
}

pub(super) fn parse_remote(url: &str) -> Result<Repository, Error> {
    let invalid = || {
        Error::Message("Pull requests require a GitHub remote (https://github.com/owner/repo or git@github.com:owner/repo)".into())
    };
    crate::git::valid_url(url).map_err(|_| invalid())?;
    if url.starts_with("https://") || url.starts_with("ssh://") {
        let parsed = reqwest::Url::parse(url).map_err(|_| invalid())?;
        if parsed.host_str() != Some("github.com")
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.password().is_some()
            || (parsed.scheme() == "ssh" && parsed.port().is_some_and(|port| port != 22))
            || (parsed.scheme() == "https" && parsed.port().is_some())
            || (parsed.scheme() == "https" && !parsed.username().is_empty())
        {
            return Err(invalid());
        }
    } else {
        let (host, _) = url.split_once(':').ok_or_else(invalid)?;
        if !host.eq_ignore_ascii_case("git@github.com") {
            return Err(invalid());
        }
    }
    let (owner, name) = parse_manual_path(url).ok_or_else(invalid)?;
    valid_name(owner)
        .and_then(|_| valid_name(name))
        .map_err(|_| invalid())?;
    Ok(Repository {
        owner: owner.into(),
        name: name.into(),
        remote: String::new(),
    })
}

pub(super) async fn validate(path: &str, branch: &str) -> Result<(), String> {
    crate::git::valid_ref(branch)?;
    if branch.len() > 1024 {
        return Err("Branch name is too long".into());
    }
    let path = path.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        crate::git::valid_path(&path, true)?;
        if !Path::new(&path).is_dir() {
            return Err("Repository path must be a directory".into());
        }
        Ok(())
    })
    .await
    .map_err(|_| "Repository validation failed".to_string())?
}

async fn git(path: &str, args: &[&str], expected: &[i32]) -> Result<Captured, Error> {
    let mut argv = vec!["--no-optional-locks", "-C", path];
    argv.extend_from_slice(args);
    execute(
        Request {
            args: &argv,
            context: "Pull request remote",
            expected,
            policy: OutputPolicy::Metadata,
            timeout: Duration::from_secs(45),
        },
        None,
    )
    .await
    .map_err(|_| {
        Error::Message("Cannot read the Git remote; check the repository and remote access".into())
    })
}

pub(super) async fn resolve(path: &str, branch: &str) -> Result<Repository, Error> {
    let configured = git(
        path,
        &["config", "--get", &format!("branch.{branch}.remote")],
        &[0, 1],
    )
    .await?;
    let configured = String::from_utf8_lossy(&configured.stdout)
        .trim()
        .to_string();
    let remote = if configured.is_empty() {
        let remotes = git(path, &["remote"], &[0]).await?;
        let names = String::from_utf8_lossy(&remotes.stdout);
        let names: Vec<_> = names.lines().filter(|name| !name.is_empty()).collect();
        match names.as_slice() {
            [only] => only.to_string(),
            many if many.contains(&"origin") => "origin".into(),
            [] => {
                return Err(Error::Message(
                    "No remote is configured for this repository".into(),
                ))
            }
            _ => {
                return Err(Error::Message(
                    "Several remotes and no origin; configure the branch upstream first".into(),
                ))
            }
        }
    } else {
        configured
    };
    if remote == "." {
        return Err(Error::Message(
            "The branch upstream is local; configure a GitHub remote".into(),
        ));
    }
    crate::git::valid_ref(&remote).map_err(|_| Error::Message("Unsupported remote name".into()))?;
    let output = git(path, &["remote", "get-url", &remote], &[0]).await?;
    let url = String::from_utf8_lossy(&output.stdout);
    let mut repo = parse_remote(url.trim())?;
    repo.remote = remote;
    Ok(repo)
}

pub(super) async fn is_published(
    path: &str,
    repo: &Repository,
    branch: &str,
) -> Result<bool, Error> {
    let reference = format!("refs/heads/{branch}");
    let result = git(
        path,
        &[
            "ls-remote",
            "--exit-code",
            "--heads",
            "--",
            &repo.remote,
            &reference,
        ],
        &[0, 2],
    )
    .await?;
    Ok(result.code == Some(0)
        && String::from_utf8_lossy(&result.stdout).lines().any(|line| {
            line.split_once('\t')
                .is_some_and(|(_, name)| name == reference)
        }))
}
