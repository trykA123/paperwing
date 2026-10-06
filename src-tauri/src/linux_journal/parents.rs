pub(super) mod binding;
mod cleanup;
mod inventory;
mod operation;
mod publication;
mod recovery;
pub(crate) type ParentFileReference = recovery::ParentFileReference;
pub(crate) type ParentRecoveryRecord = recovery::ParentRecoveryRecord;
pub(crate) type ParentRecoveryStage = recovery::ParentRecoveryStage;
pub(super) mod record;
pub(super) mod state;
pub(super) mod support;
use super::{Error, Journal, ReadError};
use crate::linux_guard::{
    mutation::{AncestorValue, ParentCreation, ParentPlan},
    storage::PrivateDir,
};
use record::{decode, encode, ParentIntent, LIMIT};
use state::{ParentRevision, ParentState};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub(crate) struct ParentPublication {
    pub parent_record: Option<String>,
    pub created: Vec<AncestorValue>,
    pub file: super::publication::Published,
}
#[derive(Debug)]
pub(crate) struct ParentOperationError {
    pub error: Error,
    pub parent_record: Option<String>,
    pub created: Vec<AncestorValue>,
    pub uncertain: Option<u32>,
}
impl From<Error> for ParentOperationError {
    fn from(error: Error) -> Self {
        Self {
            error,
            parent_record: None,
            created: Vec::new(),
            uncertain: None,
        }
    }
}
pub(super) struct Loaded {
    pub(super) directory: PrivateDir,
    pub(super) intent: ParentIntent,
    pub(super) intent_bytes: Vec<u8>,
    pub(super) revision: ParentRevision,
}
impl Loaded {
    fn prepared(directory: PrivateDir, intent: ParentIntent, bytes: Vec<u8>) -> Self {
        let revision = ParentRevision {
            sequence: 1,
            previous: None,
            intent: super::record::hash(&bytes),
            state: ParentState::Prepared {
                created: Vec::new(),
            },
        };
        Self {
            directory,
            intent,
            intent_bytes: bytes,
            revision,
        }
    }
}
impl Journal {
    pub(super) fn load_parent(&self, id: &str) -> Result<Loaded, ReadError> {
        if !record::id_valid(id) {
            return Err(ReadError::Foreign);
        }
        self._lock.revalidate().map_err(|_| ReadError::Incomplete)?;
        let directory = self
            .directory
            .lookup(id)
            .map_err(|_| ReadError::Incomplete)?;
        let intent_bytes = directory
            .file("intent.json", false)
            .and_then(|file| file.read(LIMIT))
            .map_err(|_| ReadError::Incomplete)?;
        let intent: ParentIntent = decode(&intent_bytes)?;
        intent.validate().map_err(|_| ReadError::Incomplete)?;
        if intent.id != id {
            return Err(ReadError::Incomplete);
        }
        let revision = read_parent_chain(&directory, &intent, &super::record::hash(&intent_bytes))?;
        Ok(Loaded {
            directory,
            intent,
            intent_bytes,
            revision,
        })
    }
    pub(super) fn append_parent(
        &self,
        loaded: &mut Loaded,
        state: ParentState,
    ) -> Result<(), Error> {
        state.validate(&loaded.intent)?;
        state::transition(&loaded.revision.state, &state)?;
        let fresh = self
            .load_parent(&loaded.intent.id)
            .map_err(|_| Error::invalid("Parent record changed before revision"))?;
        if fresh.intent_bytes != loaded.intent_bytes
            || fresh.revision.fingerprint()? != loaded.revision.fingerprint()?
        {
            return Err(Error::invalid("Parent revision authority changed"));
        }
        let revision = ParentRevision {
            sequence: loaded
                .revision
                .sequence
                .checked_add(1)
                .filter(|n| *n <= loaded.intent.revisions())
                .ok_or_else(|| Error::invalid("Parent revision limit reached"))?,
            previous: Some(loaded.revision.fingerprint()?),
            intent: super::record::hash(&loaded.intent_bytes),
            state,
        };
        let bytes = encode(&revision)?;
        self._lock.revalidate()?;
        native_directory(&loaded.directory)?;
        self.write_parent_revision(loaded, &revision, &bytes)?;
        native_directory(&loaded.directory)?;
        loaded.revision = revision;
        Ok(())
    }
    fn write_parent_revision(
        &self,
        loaded: &Loaded,
        revision: &ParentRevision,
        bytes: &[u8],
    ) -> Result<(), Error> {
        let phase = match revision.state {
            ParentState::Created { .. } => Some("parentCreated"),
            ParentState::Linked { .. } => Some("parentLinked"),
            _ => None,
        };
        let Some(phase) = phase else {
            loaded
                .directory
                .write_new(&super::state::name(revision.sequence), bytes)?;
            return Ok(self.directory.sync()?);
        };
        let checkpoint =
            |suffix: &str| self.parent_checkpoint(&format!("{phase}{suffix}"), &loaded.intent.id);
        let file = loaded
            .directory
            .file(&super::state::name(revision.sequence), true)?;
        checkpoint("FileCreated")?;
        file.write(bytes)?;
        checkpoint("FileWritten")?;
        if file.read(record::LIMIT)? != bytes {
            return Err(Error::invalid("Parent revision verification failed"));
        }
        checkpoint("FileVerified")?;
        file.sync()?;
        checkpoint("FileSynced")?;
        loaded.directory.sync()?;
        checkpoint("RecordSynced")?;
        self.directory.sync()?;
        checkpoint("NamespaceSynced")
    }
    pub(super) fn prepare_parent(&self, plan: &ParentPlan) -> Result<Loaded, ParentOperationError> {
        plan.validate().map_err(Error::from)?;
        let intent = ParentIntent {
            id: crate::linux_guard::storage::unique_name("p-").map_err(Error::from)?,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::invalid("Parent recovery clock unavailable"))?
                .as_millis(),
            plan: plan.clone(),
            directory_mode: 448,
            reserved_bytes: ParentIntent::reservation(plan.missing.len())?,
        };
        intent.validate()?;
        let bytes = encode(&intent)?;
        let root = self.parent_authority(&plan.root)?;
        root.verify_parent_next(ParentCreation { plan, created: &[] })
            .map_err(Error::from)?;
        self.admit_parent(intent.reserved_bytes)?;
        self.parent_authority(&plan.root)?
            .verify_parent_next(ParentCreation { plan, created: &[] })
            .map_err(Error::from)?;
        self._lock.revalidate().map_err(Error::from)?;
        let directory =
            self.directory
                .create_child(&intent.id)
                .map_err(|error| ParentOperationError {
                    error: error.into(),
                    parent_record: Some(intent.id.clone()),
                    created: Vec::new(),
                    uncertain: None,
                })?;
        let operation =
            || self.initialize_parent(Loaded::prepared(directory, intent.clone(), bytes));
        operation().map_err(|error| ParentOperationError {
            error,
            parent_record: Some(intent.id),
            created: Vec::new(),
            uncertain: None,
        })
    }
    fn initialize_parent(&self, loaded: Loaded) -> Result<Loaded, Error> {
        native_directory(&loaded.directory)?;
        self.parent_checkpoint("parentRecord", &loaded.intent.id)?;
        loaded
            .directory
            .write_new("intent.json", &loaded.intent_bytes)?;
        self.parent_checkpoint("parentIntent", &loaded.intent.id)?;
        loaded
            .directory
            .write_new(&super::state::name(1), &encode(&loaded.revision)?)?;
        self.directory.sync()?;
        native_directory(&loaded.directory)?;
        self.parent_checkpoint("parentPrepared", &loaded.intent.id)?;
        Ok(loaded)
    }
    pub(super) fn parent_checkpoint(&self, phase: &str, id: &str) -> Result<(), Error> {
        #[cfg(test)]
        {
            if self.fault == Some(phase) {
                return Err(Error::invalid("Injected parent journal checkpoint failure"));
            }
            PARENT_JOURNAL_HOOK.with(|slot| {
                if let Some(hook) = slot.borrow_mut().as_mut() {
                    hook(phase, id)
                } else {
                    Ok(())
                }
            })?;
        }
        let _ = (phase, id);
        Ok(())
    }
}
#[cfg(test)]
type ParentJournalHook = Box<dyn FnMut(&str, &str) -> Result<(), Error>>;
#[cfg(test)]
thread_local! { pub(in crate::linux_journal) static PARENT_JOURNAL_HOOK: std::cell::RefCell<Option<ParentJournalHook>> = const { std::cell::RefCell::new(None) }; }
fn native_directory(directory: &PrivateDir) -> Result<(), Error> {
    if directory.native_size()? > 4096 {
        return Err(Error::invalid(
            "Parent record directory exceeds its reserved allowance",
        ));
    }
    Ok(())
}

fn read_parent_chain(
    directory: &PrivateDir,
    intent: &ParentIntent,
    intent_hash: &str,
) -> Result<ParentRevision, ReadError> {
    let names = directory
        .names(intent.revisions() as usize + 1)
        .map_err(|_| ReadError::Incomplete)?;
    if names.first().is_none_or(|name| name != "intent.json") {
        return Err(ReadError::Incomplete);
    }
    let mut previous = None;
    let mut latest = None::<ParentRevision>;
    for (index, name) in names.iter().skip(1).enumerate() {
        let sequence = index as u32 + 1;
        if *name != super::state::name(sequence) {
            return Err(ReadError::Incomplete);
        }
        let bytes = directory
            .file(name, false)
            .and_then(|file| file.read(LIMIT))
            .map_err(|_| ReadError::Incomplete)?;
        let revision: ParentRevision = decode(&bytes)?;
        revision
            .validate(intent)
            .map_err(|_| ReadError::Incomplete)?;
        if revision.sequence != sequence
            || revision.previous != previous
            || revision.intent != intent_hash
        {
            return Err(ReadError::Incomplete);
        }
        if let Some(before) = &latest {
            state::transition(&before.state, &revision.state).map_err(|_| ReadError::Incomplete)?;
        } else if !matches!(&revision.state, ParentState::Prepared { .. }) {
            return Err(ReadError::Incomplete);
        }
        previous = Some(revision.fingerprint().map_err(|_| ReadError::Incomplete)?);
        latest = Some(revision);
    }
    latest.ok_or(ReadError::Incomplete)
}
