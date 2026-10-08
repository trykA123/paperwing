use super::{immutable, Error};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Change {
    pub sha: String,
    pub filename: String,
    pub status: String,
    #[serde(default)]
    pub previous_filename: Option<String>,
    #[serde(default)]
    pub patch: Option<String>,
}

impl Change {
    pub(crate) fn line_counts(&self) -> Option<(u64, u64)> {
        let patch = self.patch.as_ref()?;
        let mut added = 0;
        let mut removed = 0;
        for line in patch.lines() {
            if line.starts_with('+') {
                added += 1;
            }
            if line.starts_with('-') {
                removed += 1;
            }
        }
        Some((added, removed))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Commit {
    pub sha: String,
    pub commit: CommitInfo,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CommitInfo {
    pub message: String,
    pub author: Option<Author>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Author {
    pub name: Option<String>,
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Sha {
    pub sha: String,
}

#[derive(Deserialize)]
pub(super) struct Page {
    pub base_commit: Sha,
    pub merge_base_commit: Sha,
    pub ahead_by: u64,
    pub behind_by: u64,
    pub total_commits: u64,
    pub commits: Vec<Commit>,
    #[serde(default)]
    pub files: Vec<Change>,
}

pub(super) fn valid_sha(sha: String) -> Result<String, Error> {
    if !immutable(&sha) {
        return Err(Error::Message("Invalid GitHub object ID".into()));
    }
    Ok(sha.to_ascii_lowercase())
}

pub(super) fn validate_page(page: &Page, base: &str) -> Result<(), Error> {
    valid_sha(page.merge_base_commit.sha.clone())?;
    if !page.base_commit.sha.eq_ignore_ascii_case(base) {
        return Err(Error::Message(
            "GitHub comparison returned a different base".into(),
        ));
    }
    for commit in &page.commits {
        valid_sha(commit.sha.clone())?;
    }
    for file in &page.files {
        valid_sha(file.sha.clone())?;
        if file.filename.is_empty()
            || file.filename.contains('\0')
            || file
                .previous_filename
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.contains('\0'))
        {
            return Err(Error::Message("Invalid GitHub comparison path".into()));
        }
        if !matches!(
            file.status.as_str(),
            "added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged"
        ) {
            return Err(Error::Message("Unsupported GitHub file status".into()));
        }
    }
    Ok(())
}

pub(super) fn comparison(page: &Page, refs: [String; 2]) -> super::Comparison {
    let [base, head] = refs;
    super::Comparison {
        base,
        head,
        merge_base: page.merge_base_commit.sha.clone(),
        files: page.files.iter().take(super::FILE_LIMIT).cloned().collect(),
        commits: Vec::new(),
        ahead: page.ahead_by,
        behind: page.behind_by,
        truncated: super::Truncated {
            files: page.files.len() >= super::FILE_LIMIT,
            commits: page.total_commits >= super::COMMIT_LIMIT as u64,
        },
    }
}

pub(super) fn validate_cached(data: &super::Comparison) -> Result<(), Error> {
    valid_sha(data.head.clone())?;
    if data.files.len() > super::FILE_LIMIT || data.commits.len() > super::COMMIT_LIMIT {
        return Err(Error::Message(
            "GitHub comparison cache exceeds limits".into(),
        ));
    }
    validate_page(
        &Page {
            base_commit: Sha {
                sha: data.base.clone(),
            },
            merge_base_commit: Sha {
                sha: data.merge_base.clone(),
            },
            ahead_by: data.ahead,
            behind_by: data.behind,
            total_commits: data.commits.len() as u64,
            files: data.files.clone(),
            commits: data.commits.clone(),
        },
        &data.base,
    )
}
