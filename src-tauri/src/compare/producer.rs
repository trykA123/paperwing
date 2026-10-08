use super::*;

impl Service {
    pub(super) fn transition(&self, id: &str, generation: u64, state: State) {
        let mut sessions = self.sessions();
        let Some(session) = sessions
            .get_mut(id)
            .filter(|session| session.generation == generation)
        else {
            return;
        };
        if let Some(progress) = &mut session.progress {
            progress.state = state;
            progress.emit(id, generation);
            session.notify.notify_waiters();
        }
    }

    fn worker(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            owner: false,
        }
    }

    pub(super) async fn start(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        options: Options,
        source: Option<CompareSource>,
        app: Option<tauri::AppHandle>,
    ) -> Result<Opened, Problem> {
        let (contexts, generation, job, previous, producer) = {
            let mut sessions = self.sessions();
            let session = sessions
                .get_mut(id)
                .ok_or_else(|| Problem::new("unknownSession", "Unknown comparison"))?;
            Self::rebind(settings, &session.left)?;
            Self::rebind(settings, &session.right)?;
            session.cancel.store(true, Ordering::Relaxed);
            session.notify.notify_waiters();
            session.cancel = Arc::new(AtomicBool::new(false));
            session.notify = Arc::default();
            session.generation += 1;
            session.prepared = None;
            session.remote = None;
            session.progress = Some(progressive::Retained::new(app));
            if let Some(progress) = &session.progress {
                progress.emit(id, session.generation);
            }
            let previous = std::mem::take(&mut session.readers);
            let job = self.session_job(id, session);
            (
                [session.left.clone(), session.right.clone()],
                session.generation,
                job,
                previous,
                session.producer.take(),
            )
        };
        let service = self.worker();
        let settings = crate::settings::Settings {
            sources: settings.sources.clone(),
            workspace: settings.workspace.clone(),
        };
        let id_owned = id.to_owned();
        let supervisor = tokio::spawn(async move {
            close_readers(&previous).await;
            if let Some(producer) = producer {
                let _ = producer.await;
            }
            service.reset_configuration(&contexts);
            let worker = service.worker();
            let worker_id = id_owned.clone();
            let cleanup = job.readers.clone();
            let task = tokio::spawn(async move {
                let _permit = job.slot(&worker.slots).await?;
                let produce = worker.prepare_source(
                    &settings,
                    remote::Refresh {
                        id: &worker_id,
                        generation,
                        endpoints: [contexts[0].endpoint.clone(), contexts[1].endpoint.clone()],
                        options,
                        job: &job,
                    },
                    contexts,
                    source,
                );

                tokio::select! {
                    result = produce => result,
                    _ = wait_cancel(&job.cancel) => Err(Problem::new("cancelled", "Comparison cancelled")),
                }
            });
            let result = task
                .await
                .unwrap_or_else(|_| Err(Problem::new("internal", "Comparison producer failed")));
            if result.is_err() {
                close_readers(&cleanup).await;
            }
            service.finish(&id_owned, generation, result).await;
        });
        let mut sessions = self.sessions();
        if let Some(session) = sessions
            .get_mut(id)
            .filter(|session| session.generation == generation)
        {
            session.producer = Some(supervisor);
        }
        Ok(Opened {
            id: id.into(),
            generation,
        })
    }

    pub(super) fn session_job(&self, id: &str, session: &Session) -> Job {
        Job {
            rust_counts: None,
            count_root: self.counts.get().and_then(|counts| counts.storage.clone()),
            readers: session.readers.clone(),
            context: format!("compare:{id}"),
            cancel: session.cancel.clone(),
            #[cfg(target_os = "linux")]
            diff: self.diff.get().cloned(),
            #[cfg(target_os = "linux")]
            roots: Vec::new(),
            #[cfg(test)]
            temporary_root: None,
            #[cfg(test)]
            inventory_started: None,
        }
    }

    async fn finish(&self, id: &str, generation: u64, result: Result<SourcePrepared, Problem>) {
        let remote_rows = match &result {
            Ok(SourcePrepared::Github(prepared)) => prepared.files(0, FILE_LIMIT).await,
            _ => Vec::new(),
        };
        let mut sessions = self.sessions();
        let Some(session) = sessions
            .get_mut(id)
            .filter(|session| session.generation == generation)
        else {
            return;
        };
        let Some(progress) = session.progress.as_mut() else {
            return;
        };
        match result {
            Ok(prepared) => {
                let (snapshot, rows) = match prepared {
                    SourcePrepared::Local(prepared) => {
                        let snapshot = prepared.view.clone();
                        let rows = prepared.rows.clone();
                        session.prepared = Some(Arc::new(*prepared));
                        (snapshot, rows)
                    }
                    SourcePrepared::Github(prepared) => {
                        let snapshot = prepared.view.clone();
                        session.remote = Some(Arc::new(*prepared));
                        (snapshot, remote_rows)
                    }
                };
                for row in rows.into_iter().filter(|_| progress.listed.is_none()) {
                    progress.updates.push(RowUpdate::Pending(PendingRow {
                        id: row.id.clone(),
                        path: row.path.clone(),
                        left: row.left.clone(),
                        right: row.right.clone(),
                        hint: Hint::ChangedId,
                    }));
                    progress.updates.push(RowUpdate::Final(row));
                }
                progress.totals = Some(Totals {
                    raw: snapshot.raw.clone(),
                    display: snapshot.display.clone(),
                    pending: 0,
                    rows: snapshot.file_count,
                });
                progress.snapshot = Some(snapshot);
                progress.listed = None;
                progress.state = State::Complete;
            }
            Err(problem) => {
                progress.problem = Some(problem);
                progress.state = State::Failed;
            }
        }
        progress.emit(id, generation);
        session.notify.notify_waiters();
    }

    pub(super) async fn wait(&self, id: &str, generation: u64) -> Result<RefreshResult, Problem> {
        loop {
            let notify = {
                let sessions = self.sessions();
                let session = sessions
                    .get(id)
                    .filter(|session| session.generation == generation)
                    .ok_or_else(|| Problem::new("cancelled", "Comparison cancelled"))?;
                let progress = session
                    .progress
                    .as_ref()
                    .ok_or_else(|| Problem::new("cancelled", "Comparison cancelled"))?;
                if let Some(snapshot) = &progress.snapshot {
                    return Ok(RefreshResult::Ready {
                        snapshot: Box::new(snapshot.clone()),
                    });
                }
                if let Some(problem) = &progress.problem {
                    return progressive::refresh_result(problem.clone());
                }
                session.notify.clone()
            };
            let changed = notify.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let done = {
                let sessions = self.sessions();
                sessions.get(id).is_none_or(|session| {
                    session.generation != generation
                        || session.progress.as_ref().is_none_or(|progress| {
                            matches!(progress.state, State::Complete | State::Failed)
                        })
                })
            };
            if !done {
                changed.await;
            }
        }
    }

    pub(super) fn progress(
        &self,
        id: &str,
        generation: u64,
        after: u64,
        limit: usize,
    ) -> Result<Progress, Problem> {
        let sessions = self.sessions();
        let session = sessions
            .get(id)
            .filter(|session| session.generation == generation)
            .ok_or_else(|| Problem::new("staleGeneration", "Unknown or obsolete comparison"))?;
        session
            .progress
            .as_ref()
            .ok_or_else(|| {
                Problem::unavailable(UnavailableReason::RefreshRequired, "Start comparison first")
            })?
            .page(id, generation, after, limit)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if !self.owner {
            return;
        }
        let closing = self.release_sessions();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(closing);
        }
    }
}

async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
