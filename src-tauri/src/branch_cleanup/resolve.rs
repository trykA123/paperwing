use crate::commit::quick;
use crate::git::valid_ref;
use std::collections::HashSet;

pub(super) const PROTECTED_NAMES: [&str; 2] = ["main", "master"];

#[derive(Clone, Debug)]
pub(super) struct Base {
    pub reference: String,
    pub name: String,
}

pub(super) struct Target {
    pub path: String,
    pub remote: Option<String>,
    pub base: Base,
    pub remote_base: Option<Base>,
    pub current: Option<String>,
    pub protected: HashSet<String>,
}

pub(super) async fn text(
    path: &str,
    args: &[&str],
    context: &str,
    expected: &[i32],
) -> Result<Option<String>, String> {
    let output = quick(path, args, &format!("{context}: {path}"), expected).await?;
    if output.code != Some(0) {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

async fn ref_exists(path: &str, reference: &str) -> Result<bool, String> {
    let output = quick(
        path,
        &["show-ref", "--verify", "--quiet", reference],
        &format!("Ref probe: {path}"),
        &[0, 1],
    )
    .await?;
    Ok(output.code == Some(0))
}

async fn remote_names(path: &str) -> Result<Vec<String>, String> {
    let out = text(path, &["remote"], "Remotes", &[0])
        .await?
        .unwrap_or_default();
    Ok(out
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

pub(super) fn valid_branch(name: &str) -> Result<(), String> {
    valid_ref(name).map_err(|_| format!("Invalid branch name: {name}"))?;
    if name.eq_ignore_ascii_case("head") || name.starts_with("refs/") {
        return Err(format!("Invalid branch name: {name}"));
    }
    Ok(())
}

pub(super) fn valid_oid(oid: &str) -> bool {
    crate::object_id::valid(oid)
}

pub(super) fn short(reference: &str) -> &str {
    reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/remotes/"))
        .unwrap_or(reference)
}

fn base_name(reference: &str) -> String {
    if let Some(rest) = reference.strip_prefix("refs/heads/") {
        return rest.to_string();
    }
    let rest = reference.strip_prefix("refs/remotes/").unwrap_or(reference);
    rest.split_once('/')
        .map_or(rest, |(_, name)| name)
        .to_string()
}

fn base_of(reference: String) -> Base {
    Base {
        name: base_name(&reference),
        reference,
    }
}

async fn choose_remote(
    path: &str,
    requested: Option<String>,
    required: bool,
) -> Result<Option<String>, String> {
    let remotes = remote_names(path).await?;
    if let Some(name) = requested
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        valid_ref(&name).map_err(|_| "Invalid remote name".to_string())?;
        if !remotes.contains(&name) {
            return Err(format!("There is no remote named {name}"));
        }
        return Ok(Some(name));
    }
    let chosen = match remotes.as_slice() {
        [only] => Some(only.clone()),
        many if many.iter().any(|name| name == "origin") => Some("origin".to_string()),
        _ => None,
    };
    if chosen.is_none() && required {
        return Err("Choose a remote".into());
    }
    Ok(chosen)
}

async fn remote_head(path: &str, remote: &str) -> Result<Option<String>, String> {
    let head = format!("refs/remotes/{remote}/HEAD");
    let target = text(path, &["symbolic-ref", "-q", &head], "Remote HEAD", &[0, 1]).await?;
    Ok(target.filter(|value| value.starts_with(&format!("refs/remotes/{remote}/"))))
}

async fn first_existing(path: &str, candidates: Vec<String>) -> Result<Option<Base>, String> {
    for candidate in candidates {
        if ref_exists(path, &candidate).await? {
            return Ok(Some(base_of(candidate)));
        }
    }
    Ok(None)
}

async fn resolve_base(
    path: &str,
    requested: Option<&str>,
    remote: Option<&str>,
) -> Result<Base, String> {
    if let Some(name) = requested {
        valid_branch(name)?;
        let candidates = vec![format!("refs/heads/{name}"), format!("refs/remotes/{name}")];
        return first_existing(path, candidates)
            .await?
            .ok_or_else(|| format!("Base branch {name} was not found"));
    }
    let mut candidates = Vec::new();
    if let Some(remote) = remote {
        candidates.extend(remote_head(path, remote).await?);
    }
    candidates.extend(["refs/heads/main".into(), "refs/heads/master".into()]);
    first_existing(path, candidates)
        .await?
        .ok_or_else(|| "No base branch was found; choose one".into())
}

async fn resolve_remote_base(
    path: &str,
    remote: &str,
    requested: Option<&str>,
) -> Result<Base, String> {
    let prefix = format!("refs/remotes/{remote}/");
    if let Some(name) = requested {
        let name = name.strip_prefix(&format!("{remote}/")).unwrap_or(name);
        valid_branch(name)?;
        return first_existing(path, vec![format!("{prefix}{name}")])
            .await?
            .ok_or_else(|| format!("Base branch {name} was not found on {remote}"));
    }
    let mut candidates: Vec<String> = remote_head(path, remote).await?.into_iter().collect();
    candidates.extend(PROTECTED_NAMES.map(|name| format!("{prefix}{name}")));
    first_existing(path, candidates)
        .await?
        .ok_or_else(|| "Choose a remote base".into())
}

async fn current_branch(path: &str) -> Result<Option<String>, String> {
    text(
        path,
        &["symbolic-ref", "--short", "-q", "HEAD"],
        "Current branch",
        &[0, 1],
    )
    .await
}

fn present(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl Target {
    async fn build(
        path: String,
        remote: Option<String>,
        base: Base,
        remote_base: Option<Base>,
    ) -> Result<Target, String> {
        let current = current_branch(&path).await?;
        let mut protected: HashSet<String> = PROTECTED_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect();
        protected.insert(base.name.clone());
        protected.extend(remote_base.iter().map(|found| found.name.clone()));
        protected.extend(current.clone());
        if let Some(remote) = &remote {
            protected.extend(
                remote_head(&path, remote)
                    .await?
                    .map(|target| base_name(&target)),
            );
        }
        Ok(Target {
            path,
            remote,
            base,
            remote_base,
            current,
            protected,
        })
    }

    pub(super) async fn local(path: &str, base: Option<String>) -> Result<Target, String> {
        let remote = choose_remote(path, None, false).await?;
        let base = resolve_base(path, present(base).as_deref(), remote.as_deref()).await?;
        Target::build(path.to_string(), remote, base, None).await
    }

    pub(super) async fn listing(
        path: &str,
        base: Option<String>,
        remote: Option<String>,
    ) -> Result<Target, String> {
        let remote = choose_remote(path, remote, false).await?;
        let base = present(base);
        let local = resolve_base(path, base.as_deref(), remote.as_deref()).await?;
        let remote_base = match &remote {
            Some(remote) => resolve_remote_base(path, remote, base.as_deref())
                .await
                .ok(),
            None => None,
        };
        Target::build(path.to_string(), remote, local, remote_base).await
    }

    #[allow(dead_code)]
    pub(super) async fn remote(
        path: &str,
        remote: String,
        base: Option<String>,
    ) -> Result<Target, String> {
        let remote = choose_remote(path, Some(remote), true)
            .await?
            .ok_or("Choose a remote")?;
        let found = resolve_remote_base(path, &remote, present(base).as_deref()).await?;
        Target::build(path.to_string(), Some(remote), found.clone(), Some(found)).await
    }
}
