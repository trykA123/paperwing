use crate::github::pulls::repository::Repository;
use crate::kernel::ci::*;
use chrono::DateTime;
use serde::Deserialize;

pub(super) const PROVIDER: &str = "github-actions";

#[derive(Deserialize)]
pub(super) struct Run {
    pub id: u64,
    pub workflow_id: u64,
    pub name: Option<String>,
    pub run_number: u64,
    #[serde(default = "first_attempt")]
    pub run_attempt: u64,
    pub head_branch: Option<String>,
    pub head_sha: String,
    pub event: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
    pub run_started_at: Option<String>,
    pub updated_at: String,
}

fn first_attempt() -> u64 {
    1
}

impl Run {
    pub(super) fn convert(self, repo: &Repository) -> Result<CiRun, CiError> {
        validate_url(&self.html_url, repo)?;
        let completed = (self.status == "completed").then_some(self.updated_at);
        let duration = duration(self.run_started_at.as_deref(), completed.as_deref())?;
        Ok(CiRun {
            provider: PROVIDER.into(),
            host: repo.host.clone(),
            id: identifier(self.id)?,
            pipeline_id: identifier(self.workflow_id)?,
            name: self.name.unwrap_or_else(|| "CI run".into()),
            number: self.run_number,
            attempt: self.run_attempt,
            branch: self.head_branch,
            commit: self.head_sha,
            trigger: self.event,
            status: status(&self.status, self.conclusion.as_deref()),
            url: self.html_url,
            started_at: self.run_started_at,
            completed_at: completed,
            duration_seconds: duration,
            capabilities: CiCapabilities {
                can_dispatch: false,
                can_rerun_failed: true,
            },
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Job {
    pub id: u64,
    pub run_id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

impl Job {
    pub(super) fn convert(self, repo: &Repository) -> Result<CiJob, CiError> {
        if let Some(url) = &self.html_url {
            validate_url(url, repo)?;
        }
        let duration = duration(self.started_at.as_deref(), self.completed_at.as_deref())?;
        Ok(CiJob {
            provider: PROVIDER.into(),
            host: repo.host.clone(),
            id: identifier(self.id)?,
            run_id: identifier(self.run_id)?,
            name: self.name,
            status: status(&self.status, self.conclusion.as_deref()),
            url: self.html_url,
            started_at: self.started_at,
            completed_at: self.completed_at,
            duration_seconds: duration,
            steps: self
                .steps
                .into_iter()
                .map(|step| step.convert(repo))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Step {
    pub number: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl Step {
    fn convert(self, repo: &Repository) -> Result<CiStep, CiError> {
        duration(self.started_at.as_deref(), self.completed_at.as_deref())?;
        Ok(CiStep {
            provider: PROVIDER.into(),
            host: repo.host.clone(),
            number: self.number,
            name: self.name,
            status: status(&self.status, self.conclusion.as_deref()),
            started_at: self.started_at,
            completed_at: self.completed_at,
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Artifact {
    pub id: u64,
    pub name: String,
    pub size_in_bytes: u64,
    pub expired: bool,
    pub created_at: String,
    pub expires_at: Option<String>,
}

impl Artifact {
    pub(super) fn convert(self, repo: &Repository) -> Result<CiArtifact, CiError> {
        timestamp(&self.created_at)?;
        if let Some(expires) = &self.expires_at {
            timestamp(expires)?;
        }
        Ok(CiArtifact {
            provider: PROVIDER.into(),
            host: repo.host.clone(),
            id: identifier(self.id)?,
            name: self.name,
            size_bytes: self.size_in_bytes,
            expired: self.expired,
            created_at: self.created_at,
            expires_at: self.expires_at,
        })
    }
}

fn identifier(id: u64) -> Result<String, CiError> {
    if id == 0 {
        return Err(CiError::message("Invalid CI identifier from GitHub"));
    }
    Ok(id.to_string())
}

pub(super) fn validate_id(id: &str) -> Result<(), CiError> {
    if id.parse::<u64>().is_ok_and(|value| value > 0)
        && id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Ok(());
    }
    Err(CiError::message("Invalid CI identifier"))
}

pub(super) fn validate_url(url: &str, repo: &Repository) -> Result<(), CiError> {
    let url =
        reqwest::Url::parse(url).map_err(|_| CiError::message("Invalid CI URL from GitHub"))?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(&repo.host))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(CiError::message("Invalid CI URL from GitHub"));
    }
    Ok(())
}

fn timestamp(value: &str) -> Result<DateTime<chrono::FixedOffset>, CiError> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| CiError::message("Invalid CI timestamp from GitHub"))
}

fn duration(started: Option<&str>, completed: Option<&str>) -> Result<Option<u64>, CiError> {
    let started = started.map(timestamp).transpose()?;
    let completed = completed.map(timestamp).transpose()?;
    Ok(started
        .zip(completed)
        .and_then(|(start, end)| u64::try_from((end - start).num_seconds()).ok()))
}

fn status(state: &str, conclusion: Option<&str>) -> CiStatus {
    if state == "completed" {
        return match conclusion {
            Some("success" | "neutral") => CiStatus::Succeeded,
            Some("failure" | "timed_out" | "startup_failure" | "action_required" | "stale") => {
                CiStatus::Failed
            }
            Some("cancelled") => CiStatus::Cancelled,
            Some("skipped") => CiStatus::Skipped,
            _ => CiStatus::Unknown,
        };
    }
    match state {
        "queued" | "requested" | "pending" => CiStatus::Queued,
        "in_progress" => CiStatus::Running,
        "waiting" => CiStatus::Waiting,
        _ => CiStatus::Unknown,
    }
}
