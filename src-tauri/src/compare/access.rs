use super::*;
use listing::Listed;

pub(super) enum Available {
    Complete(Arc<Prepared>),
    Enriching(Arc<Listed>, std::collections::HashSet<String>),
}

impl Available {
    pub fn side(&self, side: &str) -> Result<&Resolved, Problem> {
        let (left, right) = match self {
            Self::Complete(prepared) => (&prepared.left, &prepared.right),
            Self::Enriching(listed, _) => (&listed.left, &listed.right),
        };
        match side {
            "left" => Ok(left),
            "right" => Ok(right),
            _ => Err(Problem::new("invalidContext", "Unknown side")),
        }
    }

    pub fn rows(&self) -> Vec<(&str, &str, bool)> {
        match self {
            Self::Complete(prepared) => prepared
                .rows
                .iter()
                .map(|row| (row.id.as_str(), row.path.as_str(), true))
                .collect(),
            Self::Enriching(listed, final_ids) => listed
                .rows
                .iter()
                .map(|row| {
                    (
                        row.id.as_str(),
                        row.path.as_str(),
                        final_ids.contains(&row.id),
                    )
                })
                .collect(),
        }
    }

    pub fn selection(&self, file_id: &str) -> Result<(&str, bool), Problem> {
        match self {
            Self::Complete(prepared) => prepared
                .rows
                .iter()
                .find(|row| row.id == file_id)
                .map(|row| (row.path.as_str(), true)),
            Self::Enriching(listed, final_ids) => listed.indices.get(file_id).map(|&index| {
                (
                    listed.rows[index].path.as_str(),
                    final_ids.contains(file_id),
                )
            }),
        }
        .ok_or_else(|| Problem::new("unknownFile", "Unknown file identity"))
    }
}

impl Service {
    pub(super) fn available(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        generation: u64,
    ) -> Result<(Available, Job), Problem> {
        let sessions = self.sessions();
        let session = sessions
            .get(id)
            .filter(|session| session.generation == generation)
            .ok_or_else(|| Problem::new("staleGeneration", "Unknown or obsolete comparison"))?;
        Self::rebind(settings, &session.left)?;
        Self::rebind(settings, &session.right)?;
        let available = if let Some(prepared) = &session.prepared {
            Available::Complete(prepared.clone())
        } else {
            let progress = session
                .progress
                .as_ref()
                .filter(|progress| progress.state == State::Enriching)
                .ok_or_else(|| {
                    Problem::unavailable(
                        UnavailableReason::RefreshRequired,
                        "Refresh comparison first",
                    )
                })?;
            let listed = progress
                .listed
                .clone()
                .ok_or_else(|| Problem::new("internal", "Comparison inventory unavailable"))?;
            Available::Enriching(listed, progress.final_ids.clone())
        };
        Ok((available, self.session_job(id, session)))
    }

    pub(super) async fn selected_content(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        generation: u64,
        file_id: &str,
        side: &str,
    ) -> Result<Content, Problem> {
        let (available, job) = self.available(settings, id, generation)?;
        let _permit = job.slot(&self.interactive_slots).await?;
        let (path, _) = available.selection(file_id)?;
        let resolved = available.side(side)?;
        read_root(&resolved.context, &job).await?;
        let entry = resolved
            .files
            .get(path)
            .ok_or_else(|| Problem::new("unavailable", "File absent on selected side"))?;
        let bytes = content(resolved, path, entry, &job).await?;
        job.check()?;
        self.available(settings, id, generation)?;
        Ok(Content { bytes })
    }
}
