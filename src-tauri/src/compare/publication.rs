use super::*;
use listing::Listed;

impl Service {
    pub(super) async fn publish_inventory(
        &self,
        identity: (&str, u64),
        listed: Arc<Listed>,
    ) -> Result<(), Problem> {
        let (id, generation) = identity;
        {
            let mut sessions = self.sessions();
            let Some(session) = sessions.get_mut(id) else {
                return Ok(());
            };
            check_generation(session, generation)?;
            let Some(progress) = session.progress.as_mut() else {
                return Ok(());
            };
            progress.totals = Some(Totals {
                raw: Summary::default(),
                display: Summary::default(),
                rows: listed.rows.len(),
                pending: listed
                    .rows
                    .iter()
                    .filter(|row| {
                        [row.left.as_ref(), row.right.as_ref()]
                            .into_iter()
                            .flatten()
                            .any(|side| side.kind != Kind::Directory)
                    })
                    .count(),
            });
            progress.listed = Some(listed.clone());
            progress.state = State::Enriching;
            progress.emit(id, generation);
            session.notify.notify_waiters();
        }
        self.publish_updates(
            identity,
            listed.rows.iter().cloned().map(RowUpdate::Pending),
        )
        .await
    }

    pub(super) async fn publish_final(
        &self,
        identity: (&str, u64),
        rows: Vec<FileRow>,
    ) -> Result<(), Problem> {
        if !self.sessions().contains_key(identity.0) {
            return Ok(());
        }
        self.publish_updates(identity, rows.into_iter().map(RowUpdate::Final))
            .await
    }

    async fn publish_updates(
        &self,
        identity: (&str, u64),
        updates: impl Iterator<Item = RowUpdate>,
    ) -> Result<(), Problem> {
        let mut batch = Vec::new();
        let mut bytes = 0;
        for update in updates {
            let size = serde_json::to_vec(&update)
                .map_err(|_| Problem::new("internal", "Progress serialization failed"))?
                .len();
            if !batch.is_empty()
                && (batch.len() == progressive::PAGE_ROWS || bytes + size > progressive::PAGE_BYTES)
            {
                self.publish_batch(identity, std::mem::take(&mut batch))?;
                bytes = 0;
                tokio::task::yield_now().await;
            }
            bytes += size;
            batch.push(update);
        }
        if !batch.is_empty() {
            self.publish_batch(identity, batch)?;
        }
        Ok(())
    }

    fn publish_batch(&self, identity: (&str, u64), updates: Vec<RowUpdate>) -> Result<(), Problem> {
        let (id, generation) = identity;
        let mut sessions = self.sessions();
        let Some(session) = sessions.get_mut(id) else {
            return Ok(());
        };
        check_generation(session, generation)?;
        let Some(progress) = session.progress.as_mut() else {
            return Ok(());
        };
        for update in updates {
            if let RowUpdate::Final(row) = &update {
                if !progress.final_ids.insert(row.id.clone()) {
                    return Err(Problem::new("internal", "Comparison row committed twice"));
                }
                if let Some(totals) = &mut progress.totals {
                    if classification::counted(row) {
                        totals.pending -= 1;
                        totals.raw.add(&row.raw_status);
                        totals.display.add(&row.display_status);
                    }
                }
            }
            progress.updates.push(update);
        }
        progress.emit(id, generation);
        session.notify.notify_waiters();
        Ok(())
    }

    #[cfg(test)]
    pub(super) async fn defer_enrichment(
        &self,
        job: &Job,
        after_fixed: bool,
    ) -> Result<(), Problem> {
        let control = self.enrichment_control.lock().unwrap().clone();
        if let Some(control) = control.filter(|control| control.after_fixed == after_fixed) {
            control.listed.notify_one();
            if control.panic {
                panic!("producer test control");
            }
            tokio::select! {
                _ = control.release.notified() => {},
                _ = producer::wait_cancel(&job.cancel) => return Err(Problem::new("cancelled", "Comparison cancelled")),
            }
        }
        job.check()
    }
}

#[cfg(test)]
pub(super) struct EnrichmentControl {
    pub listed: tokio::sync::Notify,
    pub release: tokio::sync::Notify,
    pub panic: bool,
    pub after_fixed: bool,
}

fn check_generation(session: &Session, generation: u64) -> Result<(), Problem> {
    if session.generation != generation || session.cancel.load(Ordering::Relaxed) {
        return Err(Problem::new("cancelled", "Comparison cancelled"));
    }
    Ok(())
}
