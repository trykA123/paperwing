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

    #[cfg(test)]
    pub(super) async fn start(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        options: Options,
        source: Option<CompareSource>,
        sink: Option<progress_events::Sink>,
    ) -> Result<Opened, Problem> {
        self.start_request(
            settings,
            Start {
                id,
                options,
                source,
                sink,
            },
        )
        .await
    }

    pub(super) async fn start_request(
        &self,
        settings: &crate::settings::Settings,
        request: Start<'_>,
    ) -> Result<Opened, Problem> {
        let mut sessions = self.sessions();
        let session = sessions
            .get_mut(request.id)
            .ok_or_else(|| Problem::new("unknownSession", "Unknown comparison"))?;
        let (production, previous) = self.begin(settings, request, session)?;
        let opened = Opened {
            id: production.id.clone(),
            generation: production.generation,
        };
        #[cfg(test)]
        if let Some(control) = self.registration_control.lock().unwrap().clone() {
            control.arrived.wait();
            control.release.wait();
        }
        let service = self.worker();
        let supervisor = tokio::spawn(async move {
            service.supervise(production, previous).await;
        });
        session.producer = Some(supervisor);
        Ok(opened)
    }

    fn begin(
        &self,
        settings: &crate::settings::Settings,
        request: Start<'_>,
        session: &mut Session,
    ) -> Result<(Production, Previous), Problem> {
        Self::rebind(settings, &session.left)?;
        Self::rebind(settings, &session.right)?;
        session.cancel.store(true, Ordering::Relaxed);
        session.notify.notify_waiters();
        session.cancel = Arc::new(AtomicBool::new(false));
        session.notify = Arc::default();
        session.generation += 1;
        session.prepared = None;
        session.remote = None;
        session.progress = Some(progressive::Retained::new(request.sink));
        if let Some(progress) = &mut session.progress {
            progress.emit(request.id, session.generation);
        }
        let previous = Previous {
            readers: std::mem::take(&mut session.readers),
            task: session.producer.take(),
        };
        let production = Production {
            id: request.id.into(),
            generation: session.generation,
            contexts: [session.left.clone(), session.right.clone()],
            options: request.options,
            source: request.source,
            settings: crate::settings::Settings {
                sources: settings.sources.clone(),
                workspace: settings.workspace.clone(),
            },
            job: self.session_job(request.id, session),
        };
        Ok((production, previous))
    }

    async fn supervise(&self, production: Production, previous: Previous) {
        close_readers(&previous.readers).await;
        if let Some(task) = previous.task {
            let _ = task.await;
        }
        self.reset_configuration(&production.contexts);
        let (id, generation) = (production.id.clone(), production.generation);
        let cleanup = production.job.readers.clone();
        let event_service = self.worker();
        let event_id = id.clone();
        let events = tokio::spawn(async move {
            event_service.send_events(&event_id, generation).await;
        });
        let worker = self.worker();
        let task = tokio::spawn(async move { worker.produce(production).await });
        let result = task
            .await
            .unwrap_or_else(|_| Err(Problem::new("internal", "Comparison producer failed")));
        if result.is_err() {
            close_readers(&cleanup).await;
        }
        self.finish(&id, generation, result).await;
        let _ = events.await;
    }

    async fn produce(&self, production: Production) -> Result<SourcePrepared, Problem> {
        let _permit = production.job.slot(&self.slots).await?;
        self.prepare_source(
            &production.settings,
            remote::Refresh {
                id: &production.id,
                generation: production.generation,
                endpoints: production
                    .contexts
                    .each_ref()
                    .map(|context| context.endpoint.clone()),
                options: production.options,
                job: &production.job,
            },
            production.contexts,
            production.source,
        )
        .await
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

#[cfg(test)]
pub(super) async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub(super) struct Start<'a> {
    pub id: &'a str,
    pub options: Options,
    pub source: Option<CompareSource>,
    pub sink: Option<progress_events::Sink>,
}

struct Production {
    id: String,
    generation: u64,
    contexts: [Context; 2],
    options: Options,
    source: Option<CompareSource>,
    settings: crate::settings::Settings,
    job: Job,
}

struct Previous {
    readers: Arc<Mutex<Vec<git::BatchReader>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

#[cfg(test)]
pub(super) struct RegistrationControl {
    pub arrived: std::sync::Barrier,
    pub release: std::sync::Barrier,
}
