use super::super::http::Error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PullState {
    Draft,
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    ReviewRequired,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChecksState {
    Success,
    Failure,
    Pending,
    None,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    pub state: ChecksState,
    pub success: usize,
    pub failure: usize,
    pub pending: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PullState,
    pub base: String,
    pub head_sha: String,
    pub review_state: ReviewState,
    pub checks: Checks,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenPullRequest {
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
    #[serde(default = "draft_default")]
    pub draft: bool,
}

fn draft_default() -> bool {
    true
}

impl OpenPullRequest {
    pub fn validate(&self) -> Result<(), Error> {
        for branch in [&self.head, &self.base] {
            crate::git::valid_ref(branch).map_err(Error::Message)?;
            if branch.len() > 1024 {
                return Err(Error::Message(
                    "Pull request branch name is too long".into(),
                ));
            }
        }
        if self.head == self.base {
            return Err(Error::Message(
                "Choose a base branch different from the head branch".into(),
            ));
        }
        if self.title.trim().is_empty()
            || self.title.len() > 256
            || self.title.chars().any(char::is_control)
        {
            return Err(Error::Message(
                "Pull request title must contain 1–256 bytes without control characters".into(),
            ));
        }
        if self.body.len() > 65_536 || self.body.contains('\0') {
            return Err(Error::Message(
                "Pull request body is too long or contains a NUL character".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedPullRequest {
    pub number: u64,
    #[serde(rename(deserialize = "html_url"))]
    pub url: String,
}

#[derive(Deserialize)]
pub(super) struct GithubPull {
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub state: String,
    pub draft: bool,
    pub merged_at: Option<String>,
    pub closed_at: Option<String>,
    pub base: GithubBranch,
    pub head: GithubBranch,
}

impl GithubPull {
    pub fn state(&self) -> Result<PullState, Error> {
        if self.merged_at.is_some() {
            return Ok(PullState::Merged);
        }
        match self.state.as_str() {
            "closed" => Ok(PullState::Closed),
            "open" if self.draft => Ok(PullState::Draft),
            "open" => Ok(PullState::Open),
            _ => Err(Error::Message(
                "Unexpected pull request state from GitHub".into(),
            )),
        }
    }
}

#[derive(Deserialize)]
pub(super) struct GithubBranch {
    #[serde(rename = "ref")]
    pub name: String,
    pub sha: String,
}

#[derive(Deserialize)]
pub(super) struct Review {
    pub id: u64,
    pub user: Option<Reviewer>,
    pub state: String,
    pub submitted_at: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Reviewer {
    pub id: u64,
}

#[derive(Deserialize)]
pub(super) struct CheckRun {
    pub status: String,
    pub conclusion: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct CommitStatus {
    pub id: u64,
    pub context: String,
    pub state: String,
}
