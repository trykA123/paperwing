use super::{
    binding::ParentBinding,
    operation::{outcome, Stop},
    state::{ParentState, RetainReason},
    Error, Journal, Loaded, ParentOperationError, ParentPublication,
};
use crate::linux_guard::{
    metadata::Security,
    mutation::{OperationAuthority, Parent, ParentCreation, ParentPlan, Snapshot},
    storage, Root,
};
use crate::linux_journal::publication::{Published, Write};

pub(super) struct ParentWrite<'a> {
    pub plan: &'a ParentPlan,
    pub expected: &'a Snapshot,
    pub after: &'a [u8],
    pub authority: &'a mut dyn OperationAuthority,
}
impl Journal {
    pub(super) fn publish_parent_file(
        &self,
        write: ParentWrite<'_>,
        mut loaded: Option<Loaded>,
    ) -> Result<ParentPublication, ParentOperationError> {
        let ParentWrite {
            plan,
            expected,
            after,
            authority,
        } = write;
        let (root, parent, created) = self.parent_destination(plan, loaded.as_ref())?;
        let binding = loaded
            .as_mut()
            .map(|loaded| self.link_parent(loaded))
            .transpose()?;
        let security = match expected {
            Snapshot::Missing => Security::new_file(),
            Snapshot::Regular { security, .. } => security.clone(),
        };
        let file = self
            .publish_record_authorized(
                Write {
                    root: &root,
                    parent: &parent,
                    path: &plan.destination,
                    expected,
                    after,
                    security: &security,
                    id: binding.as_ref().map(|binding| binding.file_record.as_str()),
                    reverse: None,
                    binding: binding.as_ref(),
                },
                authority,
            )
            .map_err(|error| self.parent_file_error(loaded.as_mut(), error))?;
        if let Some(loaded) = loaded.as_mut() {
            self.finish_parent_link(loaded, &file)?;
        }
        Ok(ParentPublication {
            parent_record: loaded.map(|loaded| loaded.intent.id),
            created,
            file,
        })
    }
    fn parent_destination(
        &self,
        plan: &ParentPlan,
        loaded: Option<&Loaded>,
    ) -> Result<
        (
            Root,
            Parent,
            Vec<crate::linux_guard::mutation::AncestorValue>,
        ),
        ParentOperationError,
    > {
        let created =
            loaded.map_or_else(Vec::new, |loaded| loaded.revision.state.created().to_vec());
        let context = |error: Error| match loaded {
            Some(loaded) => outcome(loaded, error, None),
            None => error.into(),
        };
        let root = self.parent_authority(&plan.root).map_err(&context)?;
        let parent = root
            .planned_parent(ParentCreation {
                plan,
                created: &created,
            })
            .map_err(|error| context(error.into()))?;
        Ok((root, parent, created))
    }
    fn link_parent(&self, loaded: &mut Loaded) -> Result<ParentBinding, ParentOperationError> {
        let id = storage::unique_name("r-").map_err(|error| outcome(loaded, error.into(), None))?;
        self.append_parent(
            loaded,
            ParentState::Linking {
                created: loaded.revision.state.created().to_vec(),
                file_record: id.clone(),
            },
        )
        .map_err(|error| outcome(loaded, error, None))?;
        self.parent_checkpoint("parentLinking", &loaded.intent.id)
            .map_err(|error| outcome(loaded, error, None))?;
        ParentBinding::new(loaded, id).map_err(|error| outcome(loaded, error, None))
    }
    fn parent_file_error(&self, loaded: Option<&mut Loaded>, error: Error) -> ParentOperationError {
        let Some(loaded) = loaded else {
            return error.into();
        };
        if error.applied {
            let state = ParentState::Conflict {
                created: loaded.revision.state.created().to_vec(),
                uncertain: None,
                file_record: loaded.revision.state.file_record().map(str::to_string),
            };
            let _ = self.append_parent(loaded, state);
        } else {
            let reason = if error.record.is_some() {
                RetainReason::FilePublication
            } else {
                RetainReason::FileAdmission
            };
            self.stop_parent(
                loaded,
                Stop {
                    uncertain: None,
                    reason,
                },
            );
        }
        outcome(loaded, error, None)
    }
    fn finish_parent_link(
        &self,
        loaded: &mut Loaded,
        file: &Published,
    ) -> Result<(), ParentOperationError> {
        let mut linked = || -> Result<(), Error> {
            let file_intent = self.linked_file_hash(loaded, &file.record)?;
            self.append_parent(
                loaded,
                ParentState::Linked {
                    created: loaded.revision.state.created().to_vec(),
                    file_record: file.record.clone(),
                    file_intent,
                },
            )?;
            self.parent_checkpoint("parentLinked", &loaded.intent.id)
        };
        linked().map_err(|error| outcome(loaded, error.at(&file.record, true), None))
    }
}
