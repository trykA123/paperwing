use super::{immutable, Error, Request};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub(crate) struct BlobReference {
    pub sha: String,
    pub size: Option<u64>,
    pub mode: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
}

#[derive(Deserialize)]
struct Commit {
    tree: TreeId,
}
#[derive(Deserialize)]
struct TreeId {
    sha: String,
}
#[derive(Deserialize)]
struct Tree {
    tree: Vec<BlobReference>,
    truncated: bool,
}

pub(crate) async fn file(
    request: &Request,
    commit: &str,
    path: &str,
) -> Result<BlobReference, Error> {
    if !immutable(commit) {
        return Err(Error::Message("Invalid GitHub content commit".into()));
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() > 128
        || parts
            .iter()
            .any(|part| part.is_empty() || matches!(*part, "." | ".."))
    {
        return Err(Error::Message("Unsupported GitHub file path".into()));
    }
    let http = request.connect().await?;
    let prefix = request.repository.api_path();
    let commit: Commit = http
        .compare_get(&format!("{prefix}/git/commits/{commit}"))
        .await?
        .data;
    walk(
        &http,
        TreePath {
            prefix,
            sha: commit.tree.sha,
            parts,
        },
    )
    .await
}

struct TreePath<'a> {
    prefix: String,
    sha: String,
    parts: Vec<&'a str>,
}

async fn walk(
    http: &crate::github::http::Http<'_>,
    path: TreePath<'_>,
) -> Result<BlobReference, Error> {
    let TreePath {
        prefix,
        mut sha,
        parts,
    } = path;
    for (index, part) in parts.iter().enumerate() {
        if !immutable(&sha) {
            return Err(Error::Message("Invalid GitHub tree object ID".into()));
        }
        let tree: Tree = http
            .compare_get(&format!("{prefix}/git/trees/{sha}"))
            .await?
            .data;
        if tree.truncated {
            return Err(Error::Message(
                "GitHub tree metadata is truncated; clone to open this file".into(),
            ));
        }
        let entry = tree
            .tree
            .into_iter()
            .find(|entry| entry.path == *part)
            .ok_or_else(|| Error::Message("File is absent from the GitHub commit".into()))?;
        if !immutable(&entry.sha) {
            return Err(Error::Message("Invalid GitHub file object ID".into()));
        }
        if index == parts.len() - 1 {
            return Ok(entry);
        }
        if entry.kind != "tree" || entry.mode != "040000" {
            return Err(Error::Message(
                "GitHub content path is not a directory".into(),
            ));
        }
        sha = entry.sha;
    }
    Err(Error::Message("GitHub file metadata unavailable".into()))
}
