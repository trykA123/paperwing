use super::{lock, unique, Context, Service, CONTENT_LIMIT};
use crate::{
    compare::{Service as Comparisons, WriteContext},
    linux_guard::{
        mutation::{ParentPlan, Snapshot},
        Root,
    },
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone)]
pub(crate) struct Request {
    pub session: String,
    pub generation: u64,
    pub file_id: String,
    pub side: String,
}
#[derive(Clone)]
pub(super) struct Ticket {
    pub request: Request,
    pub plan: ParentPlan,
    pub expected: Snapshot,
    pub generation_cancel: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
    pub busy: Arc<AtomicBool>,
    pub undo_records: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditFile {
    ticket: String,
    bytes: Vec<u8>,
    exists: bool,
}
impl Request {
    pub(super) fn source_side(&self) -> Result<&'static str, String> {
        match self.side.as_str() {
            "left" => Ok("right"),
            "right" => Ok("left"),
            _ => Err("Unknown copy direction".into()),
        }
    }

    pub(super) async fn context(
        &self,
        comparisons: &Comparisons,
        settings: &crate::settings::Settings,
        fresh: bool,
    ) -> Result<WriteContext, String> {
        comparisons
            .write_context(
                settings,
                &self.session,
                self.generation,
                &self.file_id,
                &self.side,
                fresh,
            )
            .await
    }
}
pub(super) fn snapshot(root: &Root, plan: &ParentPlan) -> Result<Snapshot, String> {
    let bytes = root
        .read(&plan.destination, CONTENT_LIMIT)
        .map_err(|error| error.to_string())?;
    if !plan.missing.is_empty() {
        if bytes.is_some() {
            return Err("Destination appeared during preview; reopen the comparison".into());
        }
        return Ok(Snapshot::Missing);
    }
    let expected = root
        .parent(&plan.destination, false)
        .and_then(|parent| parent.snapshot())
        .map_err(|error| error.to_string())?;
    if expected.bytes() != bytes.as_ref().map(|file| file.bytes.as_slice()) {
        return Err("File changed during preview; reopen the comparison".into());
    }
    Ok(expected)
}
impl Ticket {
    pub(super) async fn validate(
        &self,
        operation: Context<'_>,
    ) -> Result<crate::settings::Settings, String> {
        self.check()?;
        let settings = operation.environment.load()?;
        let context = self
            .request
            .context(operation.comparisons, &settings, false)
            .await?;
        if context.root != self.plan.root.path
            || context.path != self.plan.destination
            || context.safe.linux_value()? != self.plan.root
        {
            return Err("Repository context changed; reopen the comparison".into());
        }
        Ok(settings)
    }

    pub(super) fn capture(
        request: Request,
        context: WriteContext,
        generation_cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let root = Root::reopen(&context.safe.linux_value()?).map_err(|error| error.to_string())?;
        root.probe_write().map_err(|error| error.to_string())?;
        let plan = root
            .preview_parents(&context.path)
            .map_err(|error| error.to_string())?;
        let expected = snapshot(&root, &plan)?;
        Ok(Self {
            request,
            plan,
            expected,
            generation_cancel,
            cancel: Arc::new(false.into()),
            busy: Arc::new(false.into()),
            undo_records: Vec::new(),
        })
    }
    pub(super) fn check(&self) -> Result<(), String> {
        if self.generation_cancel.load(Ordering::Acquire) || self.cancel.load(Ordering::Acquire) {
            return Err("Comparison changed or edit ticket closed; reopen the comparison".into());
        }
        Ok(())
    }
}
impl Service {
    pub(crate) async fn open(
        &self,
        operation: Context<'_>,
        request: Request,
    ) -> Result<EditFile, String> {
        let Context {
            comparisons,
            environment,
        } = operation;
        if lock(&self.tickets)?.len() >= 32 {
            return Err("Too many open editable files; close an editor first".into());
        }
        let settings = environment.load()?;
        let context = request.context(comparisons, &settings, true).await?;
        let generation_cancel = comparisons
            .write_revocation(&settings, &request.session, request.generation)
            .await?;
        let entry = Ticket::capture(request, context, generation_cancel)?;
        let id = unique()?;
        let result = EditFile {
            ticket: id.clone(),
            bytes: entry.expected.bytes().unwrap_or_default().to_vec(),
            exists: entry.expected.bytes().is_some(),
        };
        let mut tickets = lock(&self.tickets)?;
        if tickets.len() >= 32 {
            return Err("Too many open editable files; close an editor first".into());
        }
        entry.check()?;
        tickets.insert(id, entry);
        Ok(result)
    }
    pub(crate) fn close(&self, id: &str) -> Result<bool, String> {
        let Some(ticket) = lock(&self.tickets)?.remove(id) else {
            return Ok(false);
        };
        ticket.cancel.store(true, Ordering::Release);
        Ok(true)
    }
}
