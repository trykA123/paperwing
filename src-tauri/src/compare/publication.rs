use super::*;
use listing::Listed;

impl Service {
    pub(super) fn publish_inventory(
        &self,
        identity: (&str, u64),
        listed: Arc<Listed>,
    ) -> Result<(), Problem> {
        let (id, generation) = identity;
        let mut sessions = self.sessions();
        let Some(session) = sessions.get_mut(id) else {
            return Ok(());
        };
        if session.generation != generation || session.cancel.load(Ordering::Relaxed) {
            return Err(Problem::new("cancelled", "Comparison cancelled"));
        }
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
        progress
            .updates
            .extend(listed.rows.iter().cloned().map(RowUpdate::Pending));
        progress.listed = Some(listed);
        progress.state = State::Enriching;
        progress.emit(id, generation);
        session.notify.notify_waiters();
        Ok(())
    }

    pub(super) fn publish_final(
        &self,
        identity: (&str, u64),
        rows: Vec<FileRow>,
    ) -> Result<(), Problem> {
        let (id, generation) = identity;
        let mut sessions = self.sessions();
        let Some(session) = sessions.get_mut(id) else {
            return Ok(());
        };
        if session.generation != generation || session.cancel.load(Ordering::Relaxed) {
            return Err(Problem::new("cancelled", "Comparison cancelled"));
        }
        let Some(progress) = session.progress.as_mut() else {
            return Ok(());
        };
        for row in rows {
            if let Some(totals) = &mut progress.totals {
                if classification::counted(&row) {
                    totals.pending -= 1;
                    totals.raw.add(&row.raw_status);
                    totals.display.add(&row.display_status);
                }
            }
            progress.final_ids.insert(row.id.clone());
            progress.updates.push(RowUpdate::Final(row));
        }
        session.notify.notify_waiters();
        Ok(())
    }

    #[cfg(test)]
    pub(super) async fn defer_enrichment(&self, job: &Job) -> Result<(), Problem> {
        let control = self.enrichment_control.lock().unwrap().clone();
        if let Some(control) = control {
            control.listed.notify_one();
            if control.panic {
                panic!("producer test control");
            }
            control.release.notified().await;
        }
        job.check()
    }
}

#[cfg(test)]
pub(super) struct EnrichmentControl {
    pub listed: tokio::sync::Notify,
    pub release: tokio::sync::Notify,
    pub panic: bool,
}
