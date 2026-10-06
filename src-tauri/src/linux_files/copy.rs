use super::{
    authority::{Authority, Source},
    filesystem, lock, save,
    tickets::{snapshot, Request, Ticket},
    unique, Context, Record, Service, CONTENT_LIMIT, COPY_LIMIT,
};
use crate::{
    linux_guard::{
        mutation::{AncestorValue, ParentPlan},
        Root,
    },
    linux_journal::Journal,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct EntryApply<'a> {
    entry: &'a Entry,
    cancel: &'a Arc<AtomicBool>,
    created: &'a [(std::path::PathBuf, AncestorValue)],
}

pub(super) struct Entry {
    ticket: Ticket,
    source: Option<Source>,
    bytes: Vec<u8>,
}
impl Entry {
    async fn capture(
        operation: Context<'_>,
        request: Request,
        content: &mut impl AsyncContent,
    ) -> Result<Self, String> {
        let source_side = request.source_side()?;
        let settings = operation.environment.load()?;
        let context = request
            .context(operation.comparisons, &settings, true)
            .await?;
        let generation_cancel = operation
            .comparisons
            .write_revocation(&settings, &request.session, request.generation)
            .await?;
        let ticket = Ticket::capture(request, context, generation_cancel)?;
        let bytes = content.read(&ticket.request.file_id, source_side).await?;
        if bytes.len() > CONTENT_LIMIT {
            return Err("Copy file exceeds 2 MiB; select a smaller file".into());
        }
        let source_context = operation
            .comparisons
            .copy_source_context(
                &settings,
                &ticket.request.session,
                ticket.request.generation,
                &ticket.request.file_id,
                source_side,
            )
            .await?;
        let source = source_context
            .map(|context| capture_source(context, &ticket, &bytes))
            .transpose()?;
        ticket.check()?;
        let bytes = if source.is_some() { Vec::new() } else { bytes };
        Ok(Self {
            ticket,
            source,
            bytes,
        })
    }

    fn preview(&self) -> Result<PreviewFile, String> {
        Ok(PreviewFile {
            path: self.ticket.plan.destination.clone(),
            action: if self.ticket.expected.bytes().is_some() {
                "overwrite"
            } else {
                "create"
            },
            bytes: self.bytes()?.len(),
        })
    }

    fn bytes(&self) -> Result<&[u8], String> {
        match &self.source {
            Some(source) => source
                .expected
                .bytes()
                .ok_or_else(|| "Copy source is absent; destination retained".into()),
            None => Ok(&self.bytes),
        }
    }
}
pub(super) struct Plan {
    entries: Vec<Entry>,
    pub started: AtomicBool,
    pub cancel: Arc<AtomicBool>,
}
impl Plan {
    fn is_live(&self) -> bool {
        self.started.load(Ordering::Acquire)
            || self
                .entries
                .iter()
                .all(|entry| entry.ticket.check().is_ok())
    }
}
struct Scope {
    ids: Vec<String>,
    retained: usize,
    generation_cancel: Arc<AtomicBool>,
}
impl Scope {
    async fn capture(operation: Context<'_>, request: &Request) -> Result<Self, String> {
        let settings = operation.environment.load()?;
        let (ids, retained) = operation
            .comparisons
            .copy_ids(
                &settings,
                &request.session,
                request.generation,
                &request.file_id,
                request.source_side()?,
            )
            .await?;
        let generation_cancel = operation
            .comparisons
            .write_revocation(&settings, &request.session, request.generation)
            .await?;
        Ok(Self {
            ids,
            retained,
            generation_cancel,
        })
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preview {
    id: String,
    files: Vec<PreviewFile>,
    retained: usize,
}
#[derive(Debug, Serialize)]
pub(crate) struct PreviewFile {
    path: String,
    action: &'static str,
    bytes: usize,
}
#[derive(Debug, Serialize)]
pub(crate) struct Outcome {
    path: String,
    state: &'static str,
    record: Option<Record>,
    error: Option<String>,
}
impl Outcome {
    fn result(
        path: String,
        result: Result<save::Mutation, save::Failure>,
    ) -> (Self, Vec<AncestorValue>) {
        match result {
            Ok(mutation) => (
                Self {
                    path,
                    state: "applied",
                    record: Some(mutation.record),
                    error: None,
                },
                mutation.created,
            ),
            Err(error) => (
                Self {
                    path,
                    state: "failed",
                    record: error.record.map(|record| *record),
                    error: Some(error.message),
                },
                error
                    .published
                    .map_or_else(Vec::new, |published| published.created),
            ),
        }
    }

    fn not_attempted(entry: &Entry) -> Self {
        Self {
            path: entry.ticket.plan.destination.clone(),
            state: "notAttempted",
            record: None,
            error: Some("Cancelled or stopped after a failure; destination retained".into()),
        }
    }
}

fn rebase(plan: &ParentPlan, created: &[AncestorValue]) -> Result<ParentPlan, String> {
    let root = Root::reopen(&plan.root).map_err(|error| error.to_string())?;
    let current = root
        .preview_parents(&plan.destination)
        .map_err(|error| error.to_string())?;
    if current.existing.len() < plan.existing.len()
        || !current.existing.starts_with(&plan.existing)
        || current.existing[plan.existing.len()..]
            .iter()
            .any(|entry| !created.contains(entry))
        || plan
            .missing
            .get(current.existing.len() - plan.existing.len()..)
            .is_none_or(|rest| current.missing != rest)
    {
        return Err("Destination parents changed after preview; destination retained".into());
    }
    Ok(current)
}
fn capture_source(
    context: crate::compare::WriteContext,
    ticket: &Ticket,
    bytes: &[u8],
) -> Result<Source, String> {
    let root_value = context.safe.linux_value()?;
    if root_value.path == ticket.plan.root.path && context.path == ticket.plan.destination {
        return Err("Source and destination are the same file".into());
    }
    let root = Root::reopen(&root_value).map_err(|error| error.to_string())?;
    let plan = root
        .preview_parents(&context.path)
        .map_err(|error| error.to_string())?;
    let expected = snapshot(&root, &plan)?;
    if expected.bytes() != Some(bytes) {
        return Err("Source changed during preview; preview again".into());
    }
    Ok(Source {
        root: root_value,
        path: context.path,
        expected,
    })
}
impl Service {
    pub(crate) async fn preview(
        &self,
        operation: Context<'_>,
        request: Request,
        mut content: impl AsyncContent,
    ) -> Result<Preview, String> {
        if self.pending_copies()? >= 4 {
            return Err("Too many pending copy previews; cancel one first".into());
        }
        let scope = Scope::capture(operation, &request).await?;
        let mut entries = Vec::new();
        let mut files = Vec::new();
        let mut total = 0;
        for file_id in scope.ids {
            let item = Request {
                file_id: file_id.clone(),
                ..request.clone()
            };
            let entry = Entry::capture(operation, item, &mut content).await?;
            total += entry.bytes()?.len() + entry.ticket.expected.bytes().map_or(0, <[u8]>::len);
            if total > COPY_LIMIT {
                return Err("Copy preview exceeds 32 MiB; select a smaller scope".into());
            }
            files.push(entry.preview()?);
            entries.push(entry);
        }
        let id = self.remember(entries, &scope.generation_cancel)?;
        Ok(Preview {
            id,
            files,
            retained: scope.retained,
        })
    }

    fn pending_copies(&self) -> Result<usize, String> {
        let mut copies = lock(&self.copies)?;
        copies.retain(|_, plan| plan.is_live());
        Ok(copies.len())
    }

    fn remember(
        &self,
        entries: Vec<Entry>,
        generation_cancel: &AtomicBool,
    ) -> Result<String, String> {
        let mut copies = lock(&self.copies)?;
        copies.retain(|_, plan| plan.is_live());
        if copies.len() >= 4 {
            return Err("Too many pending copy previews; cancel one first".into());
        }
        let id = unique()?;
        if generation_cancel.load(Ordering::Acquire) {
            return Err("Comparison changed; preview again".into());
        }
        copies.insert(
            id.clone(),
            Arc::new(Plan {
                entries,
                started: false.into(),
                cancel: Arc::new(false.into()),
            }),
        );
        Ok(id)
    }
    pub(crate) fn cancel(&self, id: &str) -> Result<bool, String> {
        let mut copies = lock(&self.copies)?;
        let Some(plan) = copies.get(id) else {
            return Ok(false);
        };
        plan.cancel.store(true, Ordering::Release);
        if !plan.started.load(Ordering::Acquire) {
            copies.remove(id);
        }
        Ok(true)
    }
    pub(crate) async fn apply(
        &self,
        operation: Context<'_>,
        id: &str,
        confirmed: bool,
    ) -> Result<Vec<Outcome>, String> {
        if !confirmed {
            return Err("Byte copies require explicit confirmation".into());
        }
        let plan = lock(&self.copies)?
            .get(id)
            .cloned()
            .ok_or("Unknown or cancelled copy preview")?;
        if plan.started.swap(true, Ordering::AcqRel) {
            return Err("Copy preview was already submitted".into());
        }
        let mut outcomes = Vec::new();
        let mut stopped = false;
        let mut created = Vec::<(std::path::PathBuf, AncestorValue)>::new();
        for entry in &plan.entries {
            if stopped || plan.cancel.load(Ordering::Acquire) {
                outcomes.push(Outcome::not_attempted(entry));
                continue;
            }
            let result = self
                .apply_entry(
                    operation,
                    EntryApply {
                        entry,
                        cancel: &plan.cancel,
                        created: &created,
                    },
                )
                .await;
            stopped = result.is_err();
            let (outcome, parents) = Outcome::result(entry.ticket.plan.destination.clone(), result);
            created.extend(
                parents
                    .into_iter()
                    .map(|value| (entry.ticket.plan.root.path.clone(), value)),
            );
            outcomes.push(outcome);
        }
        lock(&self.copies)?.remove(id);
        Ok(outcomes)
    }
    async fn apply_entry(
        &self,
        operation: Context<'_>,
        batch: EntryApply<'_>,
    ) -> Result<save::Mutation, save::Failure> {
        let environment = operation.environment;
        let EntryApply {
            entry,
            cancel,
            created,
        } = batch;
        let settings = entry.ticket.validate(operation).await?;
        let _filesystem = filesystem()?;
        let own_created = created
            .iter()
            .filter(|(root, _)| root == &entry.ticket.plan.root.path)
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();
        let rebased = rebase(&entry.ticket.plan, &own_created)?;
        let mut roots = vec![entry.ticket.plan.root.clone()];
        if let Some(source) = &entry.source {
            roots.push(source.root.clone());
        }
        let journal = Journal::open_guarded(&environment.app_data, &roots)
            .map_err(|error| error.to_string())?;
        let mut authority = Authority {
            ticket: &entry.ticket,
            environment,
            settings: &settings,
            source: entry.source.as_ref(),
            cancel: Some(cancel),
        };
        save::replace(
            &journal,
            save::Write {
                ticket: &entry.ticket,
                plan: &rebased,
                bytes: entry.bytes()?,
            },
            &mut authority,
        )
    }
}
pub(crate) trait AsyncContent {
    async fn read(&mut self, file_id: &str, side: &str) -> Result<Vec<u8>, String>;
}
