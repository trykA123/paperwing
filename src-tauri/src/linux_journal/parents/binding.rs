use super::{state::ParentState, Error, Journal, Loaded};
use crate::linux_guard::{
    mutation::{Parent, ParentCreation, Snapshot, StageProof},
    Error as GuardError, ErrorKind,
};
use crate::linux_journal::{
    record::Intent,
    state::{Revision, State},
};

#[derive(Clone, Debug)]
pub(in crate::linux_journal) struct ParentBinding {
    pub parent_record: String,
    pub revision_fingerprint: String,
    pub file_record: String,
}
impl ParentBinding {
    pub(super) fn new(loaded: &Loaded, file_record: String) -> Result<Self, Error> {
        Ok(Self {
            parent_record: loaded.intent.id.clone(),
            revision_fingerprint: loaded.revision.fingerprint()?,
            file_record,
        })
    }
}
pub(in crate::linux_journal) fn guard_error(error: Error) -> GuardError {
    GuardError {
        kind: ErrorKind::Conflict,
        message: error.message,
        code: error.code,
    }
}
pub(in crate::linux_journal) struct FinalPublication<'a> {
    pub binding: Option<&'a ParentBinding>,
    pub intent: &'a Intent,
    pub intent_hash: &'a str,
    pub revision: &'a Revision,
    pub parent: &'a Parent,
    pub expected: &'a Snapshot,
    pub proof: &'a StageProof,
}
pub(super) fn matches(loaded: &Loaded, intent: &Intent) -> Result<(), Error> {
    let ancestors = loaded
        .intent
        .plan
        .existing
        .iter()
        .chain(loaded.revision.state.created())
        .cloned()
        .collect::<Vec<_>>();
    if loaded.intent.plan.root != intent.root
        || loaded.intent.plan.destination != intent.path
        || ancestors != intent.ancestors
    {
        return Err(Error::invalid(
            "Parent and file publication authority differs",
        ));
    }
    Ok(())
}
impl Journal {
    pub(in crate::linux_journal) fn verify_parent_binding(
        &self,
        binding: &ParentBinding,
    ) -> Result<Loaded, Error> {
        self._lock.revalidate()?;
        let loaded = self
            .load_parent(&binding.parent_record)
            .map_err(|_| Error::invalid("Bound parent record is not verified"))?;
        if loaded.revision.fingerprint()? != binding.revision_fingerprint
            || !matches!(&loaded.revision.state, ParentState::Linking { file_record, .. } if *file_record == binding.file_record)
            || !crate::linux_journal::record::id_valid(&binding.file_record)
            || self
                .directory
                .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)?
                .contains(&format!("parent-cleanup-{}.json", binding.parent_record))
        {
            return Err(Error::invalid("Parent publication binding changed"));
        }
        let root = self.parent_authority(&loaded.intent.plan.root)?;
        root.verify_parent_prefix(ParentCreation {
            plan: &loaded.intent.plan,
            created: loaded.revision.state.created(),
        })?;
        self._lock.revalidate()?;
        Ok(loaded)
    }
    pub(in crate::linux_journal) fn verify_parent_write(
        &self,
        binding: &ParentBinding,
        intent: &Intent,
    ) -> Result<(), Error> {
        let loaded = self.verify_parent_binding(binding)?;
        if intent.id != binding.file_record || intent.reverse_of.is_some() {
            return Err(Error::invalid("Parent file record authority differs"));
        }
        matches(&loaded, intent)
    }
    pub(in crate::linux_journal) fn verify_final_publication(
        &self,
        input: FinalPublication<'_>,
    ) -> Result<(), GuardError> {
        let verify = || -> Result<(), Error> {
            self.parent_final_hook()?;
            let file = self
                .load(&input.intent.id)
                .map_err(|_| Error::invalid("Authorized file record changed"))?;
            if file.intent_hash != input.intent_hash
                || file.revision.fingerprint()? != input.revision.fingerprint()?
                || !matches!(&file.revision.state, State::Replacing { proof } if proof == input.proof)
            {
                return Err(Error::invalid("Authorized replacing revision changed"));
            }
            self.parent_authority(&input.intent.root)?;
            input.parent.matches_ancestors(&input.intent.ancestors)?;
            input.parent.validate(input.expected)?;
            input
                .parent
                .verify_stage(input.proof, &file.after, &file.intent.after_security)?;
            if let Some(binding) = input.binding {
                self.verify_parent_write(binding, &file.intent)?;
            }
            self._lock.revalidate()?;
            Ok(())
        };
        verify().map_err(guard_error)
    }
    pub(super) fn linked_file_hash(&self, parent: &Loaded, id: &str) -> Result<String, Error> {
        let file = self
            .load(id)
            .map_err(|_| Error::invalid("Linked file record is not verified"))?;
        matches(parent, &file.intent)?;
        if !file.revision.state.terminal() || file.intent.id != id {
            return Err(Error::invalid("Linked file state is not terminal"));
        }
        Ok(file.intent_hash)
    }
    fn parent_final_hook(&self) -> Result<(), Error> {
        #[cfg(test)]
        return FINAL_HOOK.with(|slot| {
            let mut hook = slot.borrow_mut().take();
            if let Some(hook) = hook.as_mut() {
                hook()?;
            }
            Ok(())
        });
        #[cfg(not(test))]
        Ok(())
    }
}
#[cfg(test)]
type FinalHook = Box<dyn FnMut() -> Result<(), Error>>;
#[cfg(test)]
thread_local! { pub(in crate::linux_journal) static FINAL_HOOK: std::cell::RefCell<Option<FinalHook>> = const { std::cell::RefCell::new(None) }; }
