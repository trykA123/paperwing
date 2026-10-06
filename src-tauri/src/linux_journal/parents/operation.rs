use super::publication::ParentWrite;
use super::{
    binding::guard_error,
    state::{ParentState, RetainReason},
    Error, Journal, Loaded, ParentOperationError, ParentPublication,
};
use crate::linux_guard::{
    mutation::{AncestorValue, OperationAuthority, ParentCreation, ParentPlan, Snapshot},
    Root,
};
use std::cell::Cell;

pub(super) struct Stop {
    pub uncertain: Option<u32>,
    pub reason: RetainReason,
}

struct ComponentError {
    error: crate::linux_guard::Error,
    applied: bool,
    cancelled: bool,
}

struct TrackedAuthority<'a> {
    caller: &'a mut dyn OperationAuthority,
    denied: Cell<bool>,
}
impl OperationAuthority for TrackedAuthority<'_> {
    fn refresh(&mut self) -> Result<(), crate::linux_guard::Error> {
        let result = self.caller.refresh();
        if result.is_err() {
            self.denied.set(true);
        }
        result
    }
    fn check(&self) -> Result<(), crate::linux_guard::Error> {
        let result = self.caller.check();
        if result.is_err() {
            self.denied.set(true);
        }
        result
    }
}
pub(super) fn outcome(
    loaded: &Loaded,
    error: Error,
    uncertain: Option<u32>,
) -> ParentOperationError {
    ParentOperationError {
        error,
        parent_record: Some(loaded.intent.id.clone()),
        created: loaded.revision.state.created().to_vec(),
        uncertain,
    }
}
impl Journal {
    pub(crate) fn replace_with_parents(
        &self,
        plan: &ParentPlan,
        expected: &Snapshot,
        after: &[u8],
        authority: &mut impl OperationAuthority,
    ) -> Result<ParentPublication, ParentOperationError> {
        #[cfg(test)]
        if !self.writer {
            return Err(Error::invalid("New Linux writer disabled").into());
        }
        plan.validate().map_err(Error::from)?;
        authority.refresh().map_err(Error::from)?;
        let root = self.parent_authority(&plan.root)?;
        root.verify_parent_prefix(ParentCreation { plan, created: &[] })
            .map_err(Error::from)?;
        if after.len() > crate::linux_guard::FILE_LIMIT
            || (!plan.missing.is_empty() && !matches!(expected, Snapshot::Missing))
        {
            return Err(
                Error::invalid("Parent file snapshot or content limit is unsupported").into(),
            );
        }
        authority.check().map_err(Error::from)?;
        if plan.missing.is_empty() {
            return self.publish_parent_file(
                ParentWrite {
                    plan,
                    expected,
                    after,
                    authority,
                },
                None,
            );
        }
        let mut loaded = self.prepare_parent(plan)?;
        self.create_planned_parents(&root, &mut loaded, authority)?;
        self.publish_parent_file(
            ParentWrite {
                plan,
                expected,
                after,
                authority,
            },
            Some(loaded),
        )
    }
    fn create_planned_parents(
        &self,
        root: &Root,
        loaded: &mut Loaded,
        authority: &mut dyn OperationAuthority,
    ) -> Result<(), ParentOperationError> {
        while loaded.revision.state.created().len() < loaded.intent.plan.missing.len() {
            self.create_recorded_component(root, loaded, authority)?;
        }
        let created = loaded.revision.state.created().to_vec();
        root.verify_parent_prefix(ParentCreation {
            plan: &loaded.intent.plan,
            created: &created,
        })
        .map_err(|error| outcome(loaded, error.into(), None))?;
        self.append_parent(loaded, ParentState::Ready { created })
            .map_err(|error| outcome(loaded, error, None))?;
        self.parent_checkpoint("parentReady", &loaded.intent.id)
            .map_err(|error| outcome(loaded, error, None))
    }
    fn prepare_creating(
        &self,
        root: &Root,
        loaded: &mut Loaded,
    ) -> Result<(), ParentOperationError> {
        let mut prepare = || -> Result<(), Error> {
            self.parent_authority(&loaded.intent.plan.root)?;
            root.verify_parent_next(ParentCreation {
                plan: &loaded.intent.plan,
                created: loaded.revision.state.created(),
            })?;
            self.append_parent(
                loaded,
                ParentState::Creating {
                    created: loaded.revision.state.created().to_vec(),
                    next: loaded.revision.state.created().len() as u32,
                },
            )?;
            self.parent_checkpoint("parentCreating", &loaded.intent.id)
        };
        prepare().map_err(|error| outcome(loaded, error, None))
    }
    fn create_recorded_component(
        &self,
        root: &Root,
        loaded: &mut Loaded,
        authority: &mut dyn OperationAuthority,
    ) -> Result<(), ParentOperationError> {
        self.prepare_creating(root, loaded)?;
        let fingerprint = loaded
            .revision
            .fingerprint()
            .map_err(|error| outcome(loaded, error, None))?;
        let mut tracked = TrackedAuthority {
            caller: authority,
            denied: Cell::new(false),
        };
        let mutation = root.create_parent_component(
            ParentCreation {
                plan: &loaded.intent.plan,
                created: loaded.revision.state.created(),
            },
            &mut tracked,
            || {
                self.verify_creating(loaded, &fingerprint)
                    .map_err(guard_error)
            },
        );
        if let Some(error) = mutation.error {
            return Err(self.parent_component_error(
                loaded,
                ComponentError {
                    error,
                    applied: mutation.applied,
                    cancelled: tracked.denied.get(),
                },
            ));
        }
        let identity = mutation.identity.ok_or_else(|| {
            self.parent_component_error(
                loaded,
                ComponentError {
                    error: crate::linux_guard::Error::conflict(
                        "Created parent identity is unavailable",
                    ),
                    applied: true,
                    cancelled: false,
                },
            )
        })?;
        self.persist_created(loaded, identity)
    }
    fn parent_component_error(
        &self,
        loaded: &mut Loaded,
        failure: ComponentError,
    ) -> ParentOperationError {
        let created = loaded.revision.state.created().to_vec();
        let next = created.len() as u32;
        let observed = Root::reopen(&loaded.intent.plan.root)
            .and_then(|root| root.preview_parents(&loaded.intent.plan.destination))
            .is_ok_and(|plan| {
                plan.existing.iter().any(|entry| {
                    entry.relative
                        == std::path::Path::new(&loaded.intent.plan.missing[next as usize])
                })
            });
        let uncertain = (failure.applied || observed).then_some(next);
        self.stop_parent(
            loaded,
            Stop {
                uncertain,
                reason: if failure.cancelled {
                    RetainReason::Cancelled
                } else {
                    RetainReason::OperationStopped
                },
            },
        );
        ParentOperationError {
            error: failure.error.into(),
            parent_record: Some(loaded.intent.id.clone()),
            created,
            uncertain,
        }
    }
    fn persist_created(
        &self,
        loaded: &mut Loaded,
        identity: crate::linux_guard::Identity,
    ) -> Result<(), ParentOperationError> {
        let created = loaded.revision.state.created().to_vec();
        let next = created.len() as u32;
        let mut extended = created.clone();
        extended.push(AncestorValue {
            relative: loaded.intent.plan.missing[next as usize].clone().into(),
            identity,
        });
        if let Err(error) = self.append_parent(loaded, ParentState::Created { created: extended }) {
            self.stop_parent(
                loaded,
                Stop {
                    uncertain: Some(next),
                    reason: RetainReason::OperationStopped,
                },
            );
            return Err(ParentOperationError {
                error,
                parent_record: Some(loaded.intent.id.clone()),
                created,
                uncertain: Some(next),
            });
        }
        self.parent_checkpoint("parentCreated", &loaded.intent.id)
            .map_err(|error| outcome(loaded, error, None))
    }
    fn verify_creating(&self, loaded: &Loaded, fingerprint: &str) -> Result<(), Error> {
        self.parent_checkpoint("parentBeforeMkdir", &loaded.intent.id)?;
        let fresh = self
            .load_parent(&loaded.intent.id)
            .map_err(|_| Error::invalid("Creating parent record changed"))?;
        if fresh.intent_bytes != loaded.intent_bytes
            || fresh.revision.fingerprint()? != fingerprint
            || !matches!(fresh.revision.state, ParentState::Creating { .. })
        {
            return Err(Error::invalid("Creating parent authority changed"));
        }
        let root = self.parent_authority(&loaded.intent.plan.root)?;
        root.verify_parent_next(ParentCreation {
            plan: &loaded.intent.plan,
            created: loaded.revision.state.created(),
        })?;
        self._lock.revalidate()?;
        Ok(())
    }
    pub(super) fn stop_parent(&self, loaded: &mut Loaded, stop: Stop) {
        let Stop { uncertain, reason } = stop;
        let created = loaded.revision.state.created().to_vec();
        let file_record = loaded.revision.state.file_record().map(str::to_string);
        let prefix_valid = Root::reopen(&loaded.intent.plan.root)
            .and_then(|root| {
                root.verify_parent_prefix(ParentCreation {
                    plan: &loaded.intent.plan,
                    created: &created,
                })
            })
            .is_ok();
        let state = if uncertain.is_some() || !prefix_valid {
            ParentState::Conflict {
                created,
                uncertain,
                file_record,
            }
        } else {
            ParentState::Retained {
                created,
                file_record,
                reason,
            }
        };
        let _ = self.append_parent(loaded, state);
    }
}
