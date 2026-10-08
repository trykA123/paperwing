use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiCapabilities {
    pub can_dispatch: bool,
    pub can_rerun_failed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CiStatus {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRun {
    pub provider: String,
    pub host: String,
    pub id: String,
    pub pipeline_id: String,
    pub name: String,
    pub number: u64,
    pub attempt: u64,
    pub branch: Option<String>,
    pub commit: String,
    pub trigger: String,
    pub status: CiStatus,
    pub url: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<u64>,
    pub capabilities: CiCapabilities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiJob {
    pub provider: String,
    pub host: String,
    pub id: String,
    pub run_id: String,
    pub name: String,
    pub status: CiStatus,
    pub url: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<u64>,
    pub steps: Vec<CiStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiStep {
    pub provider: String,
    pub host: String,
    pub number: u64,
    pub name: String,
    pub status: CiStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiLog {
    pub provider: String,
    pub host: String,
    pub job_id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiArtifact {
    pub provider: String,
    pub host: String,
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub expired: bool,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiDownload {
    pub provider: String,
    pub host: String,
    pub filename: String,
    pub media_type: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CiPage<T> {
    Updated {
        items: Vec<T>,
        etag: Option<String>,
        next_page: Option<u32>,
    },
    NotModified {
        etag: Option<String>,
    },
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiQuery {
    pub page: Option<u32>,
    pub etag: Option<String>,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiDispatch {
    pub pipeline_id: String,
    pub reference: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CiInputKind {
    String,
    Boolean,
    Number,
    Choice,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiInput {
    pub name: String,
    pub description: String,
    pub kind: CiInputKind,
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiDispatchForm {
    pub provider: String,
    pub host: String,
    pub pipeline_id: String,
    pub reference: String,
    pub capabilities: CiCapabilities,
    pub inputs: Vec<CiInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiActionResult {
    pub provider: String,
    pub host: String,
    pub run_id: Option<String>,
    pub accepted: bool,
}

#[derive(Debug, Serialize, thiserror::Error)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CiError {
    #[error("{message}")]
    RateLimited { reset_at: String, message: String },
    #[error("{message}")]
    Message { message: String },
}

impl CiError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message {
            message: message.into(),
        }
    }
    pub fn unsupported() -> Self {
        Self::message("This CI provider does not support this action")
    }
}

#[cfg(test)]
mod tests;
