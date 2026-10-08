use super::{client::Client, transport::Transport};
use std::path::Path;

use crate::kernel::{
    capabilities::{CiProvider, ProviderFuture},
    ci::*,
};

impl<T: Transport> CiProvider for Client<'_, T> {
    type Request = CiDispatch;
    type Run = CiActionResult;
    type Error = CiError;

    fn start(&self, request: CiDispatch) -> ProviderFuture<'_, Result<CiActionResult, CiError>> {
        Box::pin(async move { self.dispatch(&request).await })
    }

    fn cancel<'a>(&'a self, run: &'a CiActionResult) -> ProviderFuture<'a, Result<(), CiError>> {
        Box::pin(async move {
            let id = run
                .run_id
                .as_deref()
                .ok_or_else(|| CiError::message("CI cancellation needs a run identifier"))?;
            self.cancel(id).await
        })
    }

    fn runs<'a>(
        &'a self,
        query: &'a CiQuery,
    ) -> ProviderFuture<'a, Result<CiPage<CiRun>, CiError>> {
        Box::pin(self.runs(query))
    }
    fn jobs<'a>(
        &'a self,
        run: &'a str,
        query: &'a CiQuery,
    ) -> ProviderFuture<'a, Result<CiPage<CiJob>, CiError>> {
        Box::pin(self.jobs(run, query))
    }
    fn log<'a>(&'a self, job: &'a str) -> ProviderFuture<'a, Result<CiLog, CiError>> {
        Box::pin(self.log(job))
    }
    fn artifacts<'a>(
        &'a self,
        run: &'a str,
        query: &'a CiQuery,
    ) -> ProviderFuture<'a, Result<CiPage<CiArtifact>, CiError>> {
        Box::pin(self.artifacts(run, query))
    }
    fn download_artifact<'a>(
        &'a self,
        artifact: &'a str,
        destination: &'a Path,
    ) -> ProviderFuture<'a, Result<CiDownload, CiError>> {
        Box::pin(self.download_artifact(artifact, destination))
    }
    fn download_logs<'a>(
        &'a self,
        run: &'a str,
        destination: &'a Path,
    ) -> ProviderFuture<'a, Result<CiDownload, CiError>> {
        Box::pin(self.download_logs(run, destination))
    }
    fn dispatch_form<'a>(
        &'a self,
        request: &'a CiDispatch,
    ) -> ProviderFuture<'a, Result<CiDispatchForm, CiError>> {
        Box::pin(self.dispatch_form(request))
    }
    fn rerun<'a>(
        &'a self,
        run: &'a str,
        failed_only: bool,
    ) -> ProviderFuture<'a, Result<CiActionResult, CiError>> {
        Box::pin(self.rerun(run, failed_only))
    }
    fn cancel_run<'a>(
        &'a self,
        run: &'a str,
    ) -> ProviderFuture<'a, Result<CiActionResult, CiError>> {
        Box::pin(async move {
            self.cancel(run).await?;
            Ok(self.receipt(Some(run.into())))
        })
    }
}
