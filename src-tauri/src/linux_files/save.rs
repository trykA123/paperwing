use super::{
    authority::Authority, filesystem, lock, recovery, tickets::Ticket, Context, Record, Service,
    CONTENT_LIMIT, CONTRACT_WARNING,
};
use crate::{
    linux_guard::mutation::{OperationAuthority, ParentPlan, Snapshot},
    linux_journal::Journal,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Busy(Arc<AtomicBool>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct Mutation {
    pub record: Record,
    pub expected: Snapshot,
    pub created: Vec<crate::linux_guard::mutation::AncestorValue>,
}

pub(super) struct Published {
    id: String,
    expected: Snapshot,
    pub created: Vec<crate::linux_guard::mutation::AncestorValue>,
}

pub(super) struct Failure {
    pub message: String,
    pub record: Option<Box<Record>>,
    pub published: Option<Box<Published>>,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            record: None,
            published: None,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}
pub(super) struct Write<'a> {
    pub ticket: &'a Ticket,
    pub plan: &'a ParentPlan,
    pub bytes: &'a [u8],
}

pub(super) fn replace(
    journal: &Journal,
    write: Write<'_>,
    authority: &mut impl OperationAuthority,
) -> Result<Mutation, Failure> {
    let Write {
        ticket,
        plan,
        bytes,
    } = write;
    let published = journal.replace_with_parents(plan, &ticket.expected, bytes, authority).map_err(|failure| {
        let record = failure.error.record.as_ref().and_then(|id| recovery::get(journal, id).ok().map(Box::new));
        Failure { record, published: None, message: format!("{}; file recovery: {}; parent recovery: {}; applied: {}; created parents: {}; uncertain parent: {:?}",
            failure.error, failure.error.record.as_deref().unwrap_or("none"), failure.parent_record.as_deref().unwrap_or("none"),
            failure.error.applied, failure.created.len(), failure.uncertain) }
    })?;
    let expected = Snapshot::Regular {
        identity: published.file.identity,
        bytes: bytes.to_vec(),
        security: published.file.security,
    };
    let mut record = match recovery::get(journal, &published.file.record) {
        Ok(record) => record,
        Err(error) => {
            return Err(Failure {
                message: format!(
                    "{error}; file recovery: {}; applied: true",
                    published.file.record
                ),
                record: None,
                published: Some(Box::new(Published {
                    id: published.file.record.clone(),
                    expected,
                    created: published.created,
                })),
            })
        }
    };
    record.warning = Some(match &published.parent_record {
        Some(id) => format!("{CONTRACT_WARNING} Parent creation record: {id}."),
        None => CONTRACT_WARNING.into(),
    });
    Ok(Mutation {
        record,
        expected,
        created: published.created,
    })
}
impl Service {
    fn save_ticket(&self, id: &str, bytes: &[u8]) -> Result<(Ticket, Busy), String> {
        if bytes.len() > CONTENT_LIMIT {
            return Err("File exceeds editor limit".into());
        }
        let ticket = lock(&self.tickets)?
            .get(id)
            .cloned()
            .ok_or("Unknown or closed edit ticket")?;
        if ticket.busy.swap(true, Ordering::AcqRel) {
            return Err("This file is already being saved".into());
        }
        let busy = Busy(ticket.busy.clone());
        ticket.check()?;
        if [ticket.expected.bytes().unwrap_or_default(), bytes]
            .iter()
            .any(|bytes| bytes.contains(&0) || std::str::from_utf8(bytes).is_err())
        {
            return Err("Editor saves support UTF-8 text only".into());
        }
        Ok((ticket, busy))
    }

    pub(crate) async fn save(
        &self,
        operation: Context<'_>,
        id: &str,
        bytes: Vec<u8>,
    ) -> Result<Record, String> {
        let environment = operation.environment;
        let (ticket, _busy) = self.save_ticket(id, &bytes)?;
        let settings = ticket.validate(operation).await?;
        let _filesystem = filesystem()?;
        let journal = Journal::open_guarded(
            &environment.app_data,
            std::slice::from_ref(&ticket.plan.root),
        )
        .map_err(|error| error.to_string())?;
        let mut authority = Authority {
            ticket: &ticket,
            environment,
            settings: &settings,
            source: None,
            cancel: None,
        };
        let mutation = replace(
            &journal,
            Write {
                ticket: &ticket,
                plan: &ticket.plan,
                bytes: &bytes,
            },
            &mut authority,
        )
        .map_err(|failure| {
            if let Some(published) = failure.published {
                let _ = self.saved_ticket(id, &ticket, *published);
            }
            failure.message
        })?;
        let record = mutation.record;
        self.saved_ticket(
            id,
            &ticket,
            Published {
                id: record.id.clone(),
                expected: mutation.expected,
                created: mutation.created,
            },
        )?;
        Ok(record)
    }

    fn saved_ticket(&self, id: &str, ticket: &Ticket, published: Published) -> Result<(), String> {
        let mut plan = ticket.plan.clone();
        plan.existing.extend(published.created);
        plan.missing.clear();
        let mut tickets = lock(&self.tickets)?;
        if let Some(current) = tickets.get_mut(id) {
            current.expected = published.expected;
            current.plan = plan;
            current.undo_records.push(published.id);
            if current.undo_records.len() > 32 {
                current.undo_records.remove(0);
            }
        }
        Ok(())
    }
}
