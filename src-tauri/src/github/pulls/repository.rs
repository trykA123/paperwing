use super::super::{enc, http::Error, parse_manual_path, valid_name};
use crate::git::{execute, Captured, OutputPolicy, Request};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq, Clone)]
pub(in crate::github) struct Repository {
    pub owner: String,
    pub name: String,
    pub host: String,
}

impl Repository {
    pub fn api_path(&self) -> String {
        format!("/repos/{}/{}", enc(&self.owner), enc(&self.name))
    }

    pub fn pulls_path(&self, owner: &str, branch: &str, state: &str) -> String {
        let head = enc(&format!("{owner}:{branch}"));
        format!(
            "{}/pulls?head={head}&state={state}&sort=updated&direction=desc",
            self.api_path()
        )
    }

    pub fn same_repo(&self, other: &Self) -> bool {
        self.host.eq_ignore_ascii_case(&other.host)
            && self.owner.eq_ignore_ascii_case(&other.owner)
            && self.name.eq_ignore_ascii_case(&other.name)
    }
}

fn invalid_remote() -> Error {
    Error::Message(
        "Pull requests require a GitHub remote using HTTPS, SSH or git@host:owner/repo".into(),
    )
}

pub(super) fn remote_host(url: &str) -> Result<String, Error> {
    crate::git::valid_url(url).map_err(|_| invalid_remote())?;
    if url.starts_with("https://") || url.starts_with("ssh://") {
        let parsed = reqwest::Url::parse(url).map_err(|_| invalid_remote())?;
        if parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.password().is_some()
            || (parsed.scheme() == "https" && !parsed.username().is_empty())
        {
            return Err(invalid_remote());
        }
        let host = parsed.host_str().ok_or_else(invalid_remote)?;
        super::super::valid_host(host).map_err(|_| invalid_remote())?;
        return Ok(host.to_ascii_lowercase());
    }
    let (host, _) = url.split_once(':').ok_or_else(invalid_remote)?;
    let host = host.strip_prefix("git@").ok_or_else(invalid_remote)?;
    super::super::valid_host(host).map_err(|_| invalid_remote())?;
    Ok(host.to_ascii_lowercase())
}

pub(in crate::github) fn parse_remote(url: &str, host: &str) -> Result<Repository, Error> {
    if !remote_host(url)?.eq_ignore_ascii_case(host) {
        return Err(invalid_remote());
    }
    let (owner, name) = parse_manual_path(url).ok_or_else(invalid_remote)?;
    valid_name(owner)
        .and_then(|_| valid_name(name))
        .map_err(|_| invalid_remote())?;
    Ok(Repository {
        owner: owner.into(),
        name: name.into(),
        host: host.to_ascii_lowercase(),
    })
}

pub(super) struct Branch {
    pub repo: Repository,
    pub head: String,
    pub sha: String,
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

async fn select_remote(path: &str, branch: &str) -> Result<String, Error> {
    let configured = git(
        path,
        &["config", "--get", &format!("branch.{branch}.remote")],
        &[0, 1],
    )
    .await?;
    let configured = String::from_utf8_lossy(&configured.stdout)
        .trim()
        .to_string();
    if configured.is_empty() {
        let remotes = git(path, &["remote"], &[0]).await?;
        let names = String::from_utf8_lossy(&remotes.stdout);
        let names: Vec<_> = names.lines().filter(|name| !name.is_empty()).collect();
        match names.as_slice() {
            [only] => Ok(only.to_string()),
            many if many.contains(&"origin") => Ok("origin".into()),
            [] => Err(Error::Message(
                "No remote is configured for this repository".into(),
            )),
            _ => Err(Error::Message(
                "Several remotes and no origin; configure the branch upstream first".into(),
            )),
        }
    } else {
        Ok(configured)
    }
}

pub(super) async fn resolve(path: &str, branch: &str) -> Result<Branch, Error> {
    let remote = select_remote(path, branch).await?;
    if remote == "." {
        return Err(Error::Message(
            "The branch upstream is local; configure a GitHub remote".into(),
        ));
    }
    crate::git::valid_ref(&remote).map_err(|_| Error::Message("Unsupported remote name".into()))?;
    let output = git(path, &["remote", "get-url", &remote], &[0]).await?;
    let url = String::from_utf8_lossy(&output.stdout);
    let host = remote_host(url.trim())?;
    let repo = parse_remote(url.trim(), &host)?;
    let merge = git(
        path,
        &["config", "--get", &format!("branch.{branch}.merge")],
        &[0, 1],
    )
    .await?;
    let merge = String::from_utf8_lossy(&merge.stdout);
    let head = remote_branch(branch, merge.trim())?;
    let reference = format!("refs/heads/{branch}");
    let tip = git(path, &["rev-parse", "--verify", &reference], &[0]).await?;
    let sha = String::from_utf8_lossy(&tip.stdout).trim().to_string();
    if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::Message("Cannot read the local branch commit".into()));
    }
    Ok(Branch { repo, head, sha })
}

fn remote_branch(local: &str, merge: &str) -> Result<String, Error> {
    let head = if merge.is_empty() {
        local
    } else {
        merge
            .strip_prefix("refs/heads/")
            .ok_or_else(|| Error::Message("The branch upstream must be a remote branch".into()))?
    };
    crate::git::valid_ref(head)
        .map_err(|_| Error::Message("Invalid upstream branch name".into()))?;
    Ok(head.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_merge_overrides_the_local_branch_name() {
        assert_eq!(
            remote_branch("local", "refs/heads/feature/remote").unwrap(),
            "feature/remote"
        );
        assert_eq!(remote_branch("local", "").unwrap(), "local");
        assert!(remote_branch("local", "refs/tags/v1").is_err());
        assert!(remote_branch("local", "refs/heads/-unsafe").is_err());
    }
    async fn run_git(path: &Path, args: &[&str]) -> Vec<u8> {
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    struct GitFixture(std::path::PathBuf);

    impl Drop for GitFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[tokio::test]
    async fn resolves_enterprise_remote_upstream_and_local_tip_without_remote_git_access() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = GitFixture(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("pulls-git-{}-{id}", std::process::id())),
        );
        std::fs::create_dir_all(&fixture.0).unwrap();
        run_git(&fixture.0, &["init", "--quiet"]).await;
        run_git(
            &fixture.0,
            &["symbolic-ref", "HEAD", "refs/heads/feature/local"],
        )
        .await;
        run_git(
            &fixture.0,
            &[
                "-c",
                "user.name=admin",
                "-c",
                "user.email=admin@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=.",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "fixture",
            ],
        )
        .await;
        run_git(
            &fixture.0,
            &[
                "remote",
                "add",
                "origin",
                "ssh://git@gitint.company.com:2222/admin/repo.git",
            ],
        )
        .await;
        run_git(
            &fixture.0,
            &["config", "branch.feature/local.remote", "origin"],
        )
        .await;
        run_git(
            &fixture.0,
            &[
                "config",
                "branch.feature/local.merge",
                "refs/heads/feature/remote",
            ],
        )
        .await;
        let path = fixture.0.to_str().unwrap();
        validate(path, "feature/local").await.unwrap();
        let branch = resolve(path, "feature/local").await.unwrap();
        assert_eq!(branch.repo.host, "gitint.company.com");
        assert_eq!(branch.repo.owner, "admin");
        assert_eq!(branch.repo.name, "repo");
        assert_eq!(branch.head, "feature/remote");
        let sha = run_git(&fixture.0, &["rev-parse", "feature/local"]).await;
        assert_eq!(branch.sha, String::from_utf8(sha).unwrap().trim());
        run_git(
            &fixture.0,
            &["config", "--unset", "branch.feature/local.merge"],
        )
        .await;
        assert_eq!(
            resolve(path, "feature/local").await.unwrap().head,
            "feature/local"
        );
    }
}
