use super::*;
use crate::settings::Settings;

type RemoteSession = (Arc<remote::Prepared>, Job, Arc<tokio::sync::Notify>);

impl Service {
    pub(crate) fn configure_remote(
        &self,
        cache: PathBuf,
        store: crate::store::Store,
    ) -> Result<(), String> {
        if !cache.is_absolute() {
            return Err("GitHub cache requires an absolute app cache directory".into());
        }
        self.remote_store
            .set(store)
            .map_err(|_| "GitHub store is already configured")?;
        self.remote_cache
            .set(Arc::new(crate::github::blob::Cache::new(cache)))
            .map_err(|_| "GitHub cache is already configured".into())
    }

    pub(super) async fn prepare_source(
        &self,
        settings: &Settings,
        refresh: remote::Refresh<'_>,
        contexts: [Context; 2],
        source: Option<CompareSource>,
    ) -> Result<SourcePrepared, Problem> {
        if let Some(request) =
            github_source::choose(settings, &contexts, source, refresh.job).await?
        {
            if self.remote_cache.get().is_none() {
                return Err(Problem::new(
                    "githubUnavailable",
                    "GitHub cache is not configured",
                ));
            }
            let store = self.remote_store.get().ok_or_else(|| {
                Problem::new("githubUnavailable", "GitHub store is not configured")
            })?;
            return remote::prepare(request, refresh, store)
                .await
                .map(|prepared| SourcePrepared::Github(Box::new(prepared)));
        }
        self.prepare(
            refresh.id,
            refresh.generation,
            contexts,
            refresh.options,
            refresh.job,
        )
        .await
        .map(|prepared| SourcePrepared::Local(Box::new(prepared)))
    }

    pub(super) async fn remote_content(
        &self,
        session: RemoteSession,
        file: (&str, &str),
    ) -> Result<Content, Problem> {
        let (prepared, job, notify) = session;
        let cache = self
            .remote_cache
            .get()
            .ok_or_else(|| Problem::new("githubUnavailable", "GitHub cache is not configured"))?;
        let _permit = job.slot(&self.interactive_slots).await?;
        let cancelled = notify.notified();
        tokio::pin!(cancelled);
        cancelled.as_mut().enable();
        job.check()?;
        let bytes = tokio::select! {
            biased;
            _ = &mut cancelled => return Err(Problem::new("cancelled", "Comparison cancelled")),
            result = prepared.content(cache, remote::ContentRequest { job: &job, file_id: file.0, side: file.1 }) => result?,
        };
        job.check()?;
        Ok(Content { bytes })
    }

    pub(super) async fn remote_snapshot(
        &self,
        settings: &Settings,
        id: &str,
        generation: u64,
    ) -> Result<Option<RemoteSession>, Problem> {
        let sessions = self.sessions();
        let session = sessions
            .get(id)
            .filter(|session| session.generation == generation)
            .ok_or_else(|| Problem::new("staleGeneration", "Unknown or obsolete comparison"))?;
        let Some(prepared) = &session.remote else {
            return Ok(None);
        };
        Self::rebind(settings, &session.left)?;
        Self::rebind(settings, &session.right)?;
        let current =
            github_source::bind(settings, &[session.left.clone(), session.right.clone()])?
                .ok_or_else(|| {
                    Problem::new(
                        "staleContext",
                        "GitHub comparison context changed; reopen comparison",
                    )
                })?;
        let job = Job {
            rust_counts: None,
            count_root: None,
            readers: session.readers.clone(),
            context: format!("compare:{id}"),
            cancel: session.cancel.clone(),
            #[cfg(target_os = "linux")]
            diff: None,
            #[cfg(target_os = "linux")]
            roots: Vec::new(),
            #[cfg(test)]
            temporary_root: None,
            #[cfg(test)]
            inventory_started: None,
        };
        prepared.check(&current, &job)?;
        Ok(Some((prepared.clone(), job, session.notify.clone())))
    }
}
