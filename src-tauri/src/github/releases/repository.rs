use super::super::pulls::{
    repository::{parse_remote, remote_host, Repository},
    PullsError,
};
use crate::commit::quick;

pub(super) struct AnnotatedTag {
    pub object: String,
    pub commit: String,
}

pub(super) async fn annotated_object(path: &str, tag: &str) -> Result<AnnotatedTag, PullsError> {
    crate::git::valid_root(path)?;
    crate::tags::validate_name(path, tag).await?;
    let object = crate::tags::tag_object(path, tag)
        .await?
        .ok_or_else(|| format!("There is no local tag named {tag}"))?;
    let kind = quick(path, &["cat-file", "-t", &object], "Release tag type", &[0]).await?;
    if String::from_utf8_lossy(&kind.stdout).trim() != "tag" {
        return Err("GitHub releases require an annotated tag"
            .to_string()
            .into());
    }
    let commit = quick(
        path,
        &["rev-parse", "--verify", &format!("{object}^{{commit}}")],
        "Release tag commit",
        &[0],
    )
    .await?;
    Ok(AnnotatedTag {
        object,
        commit: String::from_utf8_lossy(&commit.stdout).trim().to_string(),
    })
}

pub(super) async fn remotes(
    path: &str,
    requested: Option<&str>,
) -> Result<Vec<String>, PullsError> {
    if let Some(remote) = requested {
        crate::git::valid_ref(remote).map_err(|_| "Invalid remote name".to_string())?;
    }
    let output = quick(path, &["remote"], "Release remotes", &[0]).await?;
    let mut remotes: Vec<_> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(remote) = requested {
        if !remotes.iter().any(|name| name == remote) {
            return Err(format!("There is no remote named {remote}").into());
        }
        return Ok(vec![remote.to_string()]);
    }
    remotes.sort_by_key(|name| name != "origin");
    Ok(remotes)
}

pub(super) async fn resolve(path: &str, remote: &str) -> Result<(Repository, String), PullsError> {
    crate::git::valid_ref(remote)?;
    let output = quick(
        path,
        &["remote", "get-url", "--push", remote],
        "Release remote URL",
        &[0],
    )
    .await?;
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let host = remote_host(&url)?;
    Ok((parse_remote(&url, &host)?, url))
}

pub(super) async fn require_published(
    path: &str,
    url: &str,
    tag: &str,
    object: &str,
) -> Result<(), PullsError> {
    let reference = format!("refs/tags/{tag}");
    let output = quick(
        path,
        &["ls-remote", "--refs", "--exit-code", "--", url, &reference],
        "Release remote tag",
        &[0, 2],
    )
    .await?;
    let listing = String::from_utf8_lossy(&output.stdout);
    if !listing
        .lines()
        .any(|line| line.split_once('\t') == Some((object, reference.as_str())))
    {
        return Err(format!("Tag {tag} is not on the remote at the expected object; push it before creating a release").into());
    }
    Ok(())
}
