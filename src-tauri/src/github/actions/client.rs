use super::{
    model,
    transport::{Request, Transport},
};
use crate::github::{
    enc,
    http::{Error, Response},
    pulls::repository::Repository,
};
use crate::kernel::ci::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::Path;

pub(super) struct Client<'a, T> {
    pub(super) transport: &'a T,
    pub(super) repo: &'a Repository,
}

struct PageRequest<'a> {
    path: &'a str,
    field: &'a str,
    query: &'a CiQuery,
}

impl<T: Transport> Client<'_, T> {
    pub(super) async fn runs(&self, query: &CiQuery) -> Result<CiPage<CiRun>, CiError> {
        let mut path = format!("{}/actions/runs", self.repo.api_path());
        if let Some(branch) = &query.branch {
            crate::git::valid_ref(branch).map_err(CiError::message)?;
            path.push_str(&format!("?branch={}", enc(branch)));
        }
        self.page(
            PageRequest {
                path: &path,
                field: "workflow_runs",
                query,
            },
            model::Run::convert,
        )
        .await
    }

    pub(super) async fn jobs(&self, run: &str, query: &CiQuery) -> Result<CiPage<CiJob>, CiError> {
        model::validate_id(run)?;
        self.page(
            PageRequest {
                path: &format!(
                    "{}/actions/runs/{run}/jobs?filter=latest",
                    self.repo.api_path()
                ),
                field: "jobs",
                query,
            },
            model::Job::convert,
        )
        .await
    }

    pub(super) async fn artifacts(
        &self,
        run: &str,
        query: &CiQuery,
    ) -> Result<CiPage<CiArtifact>, CiError> {
        model::validate_id(run)?;
        self.page(
            PageRequest {
                path: &format!("{}/actions/runs/{run}/artifacts", self.repo.api_path()),
                field: "artifacts",
                query,
            },
            model::Artifact::convert,
        )
        .await
    }

    async fn page<Raw: DeserializeOwned, Item>(
        &self,
        request: PageRequest<'_>,
        convert: impl Fn(Raw, &Repository) -> Result<Item, CiError>,
    ) -> Result<CiPage<Item>, CiError> {
        let PageRequest { path, field, query } = request;
        let page = query.page.unwrap_or(1);
        if !(1..=1000).contains(&page) {
            return Err(CiError::message("Invalid CI page; use 1 to 1000"));
        }
        let separator = if path.contains('?') { '&' } else { '?' };
        let path = format!("{path}{separator}per_page=100&page={page}");
        let response = self
            .transport
            .send(Request::Metadata {
                path: &path,
                etag: query.etag.as_deref(),
            })
            .await
            .map_err(CiError::from)?;
        let etag = response
            .headers
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if response.status == 304 {
            if query.etag.is_none() {
                return Err(CiError::message(
                    "Unexpected CI response without a validator",
                ));
            }
            return Ok(CiPage::NotModified {
                etag: etag.or_else(|| query.etag.clone()),
            });
        }
        self.decode_page(response, (field, page, etag), convert)
    }

    fn decode_page<Raw: DeserializeOwned, Item>(
        &self,
        response: Response,
        request: (&str, u32, Option<String>),
        convert: impl Fn(Raw, &Repository) -> Result<Item, CiError>,
    ) -> Result<CiPage<Item>, CiError> {
        let (field, page, etag) = request;
        let response = checked(response, false)?;
        let next_page = response.next.then_some(page + 1);
        let value: Value = serde_json::from_slice(&response.body)
            .map_err(|_| CiError::message("Unexpected CI response from GitHub"))?;
        let items: Vec<Raw> = serde_json::from_value(
            value
                .get(field)
                .cloned()
                .ok_or_else(|| CiError::message("Missing CI records from GitHub"))?,
        )
        .map_err(|_| CiError::message("Unexpected CI records from GitHub"))?;
        Ok(CiPage::Updated {
            items: items
                .into_iter()
                .map(|item| convert(item, self.repo))
                .collect::<Result<_, _>>()?,
            etag,
            next_page,
        })
    }

    pub(super) async fn log(&self, job: &str) -> Result<CiLog, CiError> {
        model::validate_id(job)?;
        let path = format!("{}/actions/jobs/{job}", self.repo.api_path());
        let response = checked(
            self.transport
                .send(Request::Metadata {
                    path: &path,
                    etag: None,
                })
                .await
                .map_err(CiError::from)?,
            false,
        )?;
        let value: Value = serde_json::from_slice(&response.body)
            .map_err(|_| CiError::message("Unexpected CI job from GitHub"))?;
        if value.get("status").and_then(Value::as_str) != Some("completed") {
            return Err(CiError::message(
                "Job logs become available after the job finishes",
            ));
        }
        let path = format!("{path}/logs");
        let response = checked(
            self.transport
                .send(Request::Download { path: &path })
                .await
                .map_err(CiError::from)?,
            false,
        )?;
        let text = String::from_utf8(response.body)
            .map_err(|_| CiError::message("CI job log is not UTF-8 text"))?;
        Ok(CiLog {
            provider: model::PROVIDER.into(),
            host: self.repo.host.clone(),
            job_id: job.into(),
            text,
        })
    }

    pub(super) async fn download_artifact(
        &self,
        artifact: &str,
        destination: &Path,
    ) -> Result<CiDownload, CiError> {
        model::validate_id(artifact)?;
        let path = format!("{}/actions/artifacts/{artifact}/zip", self.repo.api_path());
        self.save(&path, destination, format!("artifact-{artifact}.zip"))
            .await
    }

    pub(super) async fn download_logs(
        &self,
        run: &str,
        destination: &Path,
    ) -> Result<CiDownload, CiError> {
        model::validate_id(run)?;
        let path = format!("{}/actions/runs/{run}/logs", self.repo.api_path());
        self.save(&path, destination, format!("run-{run}-logs.zip"))
            .await
    }

    async fn save(
        &self,
        path: &str,
        destination: &Path,
        filename: String,
    ) -> Result<CiDownload, CiError> {
        checked(
            self.transport
                .send(Request::Save { path, destination })
                .await
                .map_err(CiError::from)?,
            false,
        )?;
        let size_bytes = tokio::fs::metadata(destination)
            .await
            .map_err(|_| CiError::message("Cannot read the saved CI download"))?
            .len();
        Ok(CiDownload {
            provider: model::PROVIDER.into(),
            host: self.repo.host.clone(),
            filename,
            media_type: "application/zip".into(),
            path: destination.to_string_lossy().into_owned(),
            size_bytes,
        })
    }

    pub(super) async fn rerun(
        &self,
        run: &str,
        failed_only: bool,
    ) -> Result<CiActionResult, CiError> {
        model::validate_id(run)?;
        let action = if failed_only {
            "rerun-failed-jobs"
        } else {
            "rerun"
        };
        let path = format!("{}/actions/runs/{run}/{action}", self.repo.api_path());
        self.write(&path, None).await?;
        Ok(self.receipt(Some(run.into())))
    }

    pub(super) async fn cancel(&self, run: &str) -> Result<(), CiError> {
        model::validate_id(run)?;
        self.write(
            &format!("{}/actions/runs/{run}/cancel", self.repo.api_path()),
            None,
        )
        .await
    }

    pub(super) async fn write(&self, path: &str, body: Option<Value>) -> Result<(), CiError> {
        checked(
            self.transport
                .send(Request::Write { path, body })
                .await
                .map_err(CiError::from)?,
            true,
        )?;
        Ok(())
    }

    pub(super) fn receipt(&self, run_id: Option<String>) -> CiActionResult {
        CiActionResult {
            provider: model::PROVIDER.into(),
            host: self.repo.host.clone(),
            run_id,
            accepted: true,
        }
    }
}

pub(super) fn checked(response: Response, write: bool) -> Result<Response, CiError> {
    match response.checked() {
        Err(Error::Http { status: 401 | 403 | 404, .. }) => Err(CiError::message(format!("GitHub Actions access denied or repository not found; grant repository Actions {} permission", if write { "write" } else { "read" }))),
        Err(Error::Http { status: 422, .. }) => Err(CiError::message("GitHub rejected the CI action; check the run, ref and dispatch inputs")),
        result => result.map_err(CiError::from),
    }
}

impl From<Error> for CiError {
    fn from(error: Error) -> Self {
        match error {
            Error::RateLimited { reset_at } => Self::RateLimited {
                reset_at: reset_at.to_rfc3339(),
                message: format!("CI rate limit reached; retry after {reset_at}"),
            },
            error => Self::message(crate::git::safe(&error.to_string())),
        }
    }
}
