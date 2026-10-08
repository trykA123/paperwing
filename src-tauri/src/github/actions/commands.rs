use super::connection::{self, StoredConnection};
use crate::{
    github::pulls::{repository, source},
    kernel::{
        capabilities::{CiProvider, ProviderFuture},
        ci::*,
    },
};
use serde::Deserialize;
use std::path::Path;
use tauri::AppHandle;

#[derive(Deserialize)]
#[serde(untagged, rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum CiRepositoryRequest {
    Local { path: String, branch: String },
    Configured { source_id: String, url: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRerunRequest {
    pub run_id: String,
    pub failed_only: bool,
    pub confirmed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiCancelRequest {
    pub run_id: String,
    pub confirmed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiDispatchRequest {
    pub dispatch: CiDispatch,
    pub confirmed: bool,
}

type Provider<'a> =
    dyn CiProvider<Request = CiDispatch, Run = CiActionResult, Error = CiError> + 'a;

async fn with_provider<T>(
    app: AppHandle,
    target: CiRepositoryRequest,
    operation: impl for<'a> FnOnce(&'a Provider<'a>) -> ProviderFuture<'a, Result<T, CiError>>,
) -> Result<T, CiError> {
    let (source, repo) = match target {
        CiRepositoryRequest::Local { path, branch } => {
            repository::validate(&path, &branch)
                .await
                .map_err(CiError::message)?;
            let branch = repository::resolve(&path, &branch)
                .await
                .map_err(CiError::from)?;
            (
                source::load(app, path, &branch.repo)
                    .await
                    .map_err(CiError::message)?,
                branch.repo,
            )
        }
        CiRepositoryRequest::Configured { source_id, url } => {
            source::load_remote(app, source_id, url)
                .await
                .map_err(CiError::message)?
        }
    };
    connection::run(&StoredConnection, (&source, &repo), operation).await
}

pub(super) fn require_confirmation(confirmed: bool) -> Result<(), CiError> {
    if !confirmed {
        return Err(CiError::message("Confirm the CI action before continuing"));
    }
    Ok(())
}

#[tauri::command]
pub async fn ci_runs(
    app: AppHandle,
    target: CiRepositoryRequest,
    query: CiQuery,
) -> Result<CiPage<CiRun>, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.runs(&query).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_jobs(
    app: AppHandle,
    target: CiRepositoryRequest,
    run_id: String,
    query: CiQuery,
) -> Result<CiPage<CiJob>, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.jobs(&run_id, &query).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_job_log(
    app: AppHandle,
    target: CiRepositoryRequest,
    job_id: String,
) -> Result<CiLog, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.log(&job_id).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_artifacts(
    app: AppHandle,
    target: CiRepositoryRequest,
    run_id: String,
    query: CiQuery,
) -> Result<CiPage<CiArtifact>, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.artifacts(&run_id, &query).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_download_artifact(
    app: AppHandle,
    target: CiRepositoryRequest,
    artifact_id: String,
    destination: String,
) -> Result<CiDownload, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move {
            provider
                .download_artifact(&artifact_id, Path::new(&destination))
                .await
        })
    })
    .await
}

#[tauri::command]
pub async fn ci_download_run_logs(
    app: AppHandle,
    target: CiRepositoryRequest,
    run_id: String,
    destination: String,
) -> Result<CiDownload, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move {
            provider
                .download_logs(&run_id, Path::new(&destination))
                .await
        })
    })
    .await
}

#[tauri::command]
pub async fn ci_rerun(
    app: AppHandle,
    target: CiRepositoryRequest,
    request: CiRerunRequest,
) -> Result<CiActionResult, CiError> {
    require_confirmation(request.confirmed)?;
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.rerun(&request.run_id, request.failed_only).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_cancel(
    app: AppHandle,
    target: CiRepositoryRequest,
    request: CiCancelRequest,
) -> Result<CiActionResult, CiError> {
    require_confirmation(request.confirmed)?;
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.cancel_run(&request.run_id).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_dispatch_inputs(
    app: AppHandle,
    target: CiRepositoryRequest,
    request: CiDispatch,
) -> Result<CiDispatchForm, CiError> {
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.dispatch_form(&request).await })
    })
    .await
}

#[tauri::command]
pub async fn ci_dispatch(
    app: AppHandle,
    target: CiRepositoryRequest,
    request: CiDispatchRequest,
) -> Result<CiActionResult, CiError> {
    require_confirmation(request.confirmed)?;
    with_provider(app, target, |provider| {
        Box::pin(async move { provider.start(request.dispatch).await })
    })
    .await
}
