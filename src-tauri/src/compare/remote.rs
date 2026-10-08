use super::{
    Endpoint, FileRow, History, Job, Options, Problem, ResolvedEndpoint, Snapshot, Summary,
    UniqueCommit, NEXT,
};
use crate::github::blob::{Blob, Cache};
use crate::github::compare::{Comparison, Error, Request};
use std::sync::atomic::Ordering;
use tokio::sync::{Mutex, OnceCell};

mod content;
mod counts;
mod rows;
#[cfg(test)]
mod tests;

pub(super) use content::ContentRequest;

pub(super) struct Prepared {
    pub view: Snapshot,
    pub request: Request,
    data: Comparison,
    rows: Mutex<Vec<FileRow>>,
    blobs: Vec<[OnceCell<content::File>; 2]>,
    revision: u64,
}

pub(super) struct Refresh<'a> {
    pub id: &'a str,
    pub generation: u64,
    pub endpoints: [Endpoint; 2],
    pub options: Options,
    pub job: &'a Job,
}

pub(super) async fn prepare(
    request: Request,
    refresh: Refresh<'_>,
    store: &crate::store::Store,
) -> Result<Prepared, Problem> {
    refresh.job.check()?;
    let revision = crate::credentials::metadata_revision(&request.source)
        .map_err(|message| Problem::new("githubUnavailable", &message))?;
    let lease = crate::providers::acquire(&request.source, &request.repository.host, None)
        .map_err(|message| Problem::new("githubUnavailable", &message))?;
    let data = lease
        .run(crate::github::compare::load(&request, store))
        .await
        .map_err(|error| Problem::new("githubUnavailable", &error.to_string()))?
        .map_err(problem)?;
    refresh.job.check()?;
    Ok(Prepared::new(request, data, refresh, revision))
}

impl Prepared {
    fn new(request: Request, data: Comparison, refresh: Refresh<'_>, revision: u64) -> Self {
        let rows = rows::map(&data, &refresh.options);
        let view = rows::snapshot(&data, &rows, refresh);
        let blobs = (0..rows.len())
            .map(|_| std::array::from_fn(|_| OnceCell::new()))
            .collect();
        Self {
            view,
            request,
            data,
            rows: Mutex::new(rows),
            blobs,
            revision,
        }
    }

    pub(super) fn check(&self, current: &Request, job: &Job) -> Result<(), Problem> {
        job.check()?;
        crate::providers::ensure_enabled(&current.source)
            .map_err(|message| Problem::new("githubUnavailable", &message))?;
        let source = |request: &Request| serde_json::to_value(&request.source).ok();
        if !self.request.repository.same_repo(&current.repository)
            || source(&self.request) != source(current)
            || self.request.base != current.base
            || self.request.head != current.head
        {
            return Err(Problem::new(
                "staleContext",
                "GitHub source changed; refresh comparison",
            ));
        }
        if crate::credentials::metadata_revision(&current.source)
            .map_err(|message| Problem::new("githubUnavailable", &message))?
            != self.revision
        {
            return Err(Problem::new(
                "staleContext",
                "Source credentials changed; refresh comparison",
            ));
        }
        Ok(())
    }

    pub(super) async fn files(&self, offset: usize, limit: usize) -> Vec<FileRow> {
        self.rows
            .lock()
            .await
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    pub(super) fn commits(&self, offset: usize, limit: usize) -> Vec<UniqueCommit> {
        self.data
            .commits
            .iter()
            .skip(offset)
            .take(limit)
            .map(|commit| UniqueCommit {
                side: "right".into(),
                sha: commit.sha.clone(),
                subject: crate::git::safe(commit.commit.message.lines().next().unwrap_or_default()),
                author: crate::git::safe(
                    commit
                        .commit
                        .author
                        .as_ref()
                        .and_then(|author| author.name.as_deref())
                        .unwrap_or_default(),
                ),
                date: commit
                    .commit
                    .author
                    .as_ref()
                    .and_then(|author| author.date.clone())
                    .unwrap_or_default(),
            })
            .collect()
    }
}

pub(super) fn problem(error: Error) -> Problem {
    match error {
        Error::RateLimited { reset_at } => Problem {
            retry_at: Some(reset_at.timestamp_millis()),
            ..Problem::new("githubRateLimited", "GitHub rate limit, try again later")
        },
        Error::Http { status: 404, .. } => Problem::new(
            "githubNotFound",
            "GitHub repository or ref not found; check access and the refs",
        ),
        error => Problem::new("githubUnavailable", &error.to_string()),
    }
}
