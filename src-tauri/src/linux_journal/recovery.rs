use super::{
    publication::{Reverse, Write},
    state::State,
    Error, Journal, Loaded,
};
use crate::linux_guard::{mutation::Snapshot, storage, Root};

impl Journal {
    fn is_after(loaded: &Loaded, current: &Snapshot) -> bool {
        matches!(current,Snapshot::Regular{identity,bytes,security} if loaded.revision.state.proof().is_some_and(|proof|&proof.file==identity)
            && loaded.intent.after.matches(bytes) && &loaded.intent.after_security==security)
    }
    pub(super) fn reconcile_loaded(&self, loaded: &mut Loaded) -> Result<(), Error> {
        if loaded.revision.state.terminal()
            || matches!(loaded.revision.state, State::Conflict { .. })
        {
            return Ok(());
        }
        if matches!(loaded.revision.state, State::Undoing { .. }) {
            return self.reconcile_undo(loaded);
        }
        let current = loaded
            .intent
            .fresh_parent()
            .and_then(|parent| Ok(parent.snapshot()?));
        let proof = loaded.revision.state.proof().cloned();
        let state = match current {
            Ok(current) if current == loaded.intent.before_snapshot(&loaded.before) => {
                State::NotApplied {
                    proof,
                    candidate: loaded.revision.state.candidate().unwrap_or_default().into(),
                }
            }
            Ok(Snapshot::Regular {
                identity,
                bytes,
                security,
            }) if proof.as_ref().is_some_and(|proof| proof.file == identity)
                && loaded.intent.after.matches(&bytes)
                && loaded.intent.after_security == security =>
            {
                State::Applied {
                    proof: proof.ok_or_else(|| Error::invalid("Missing staged identity"))?,
                    identity,
                }
            }
            _ => State::Conflict { proof },
        };
        self.append(loaded, state)
    }
    fn reconcile_undo(&self, loaded: &mut Loaded) -> Result<(), Error> {
        let State::Undoing {
            proof,
            identity,
            provenance,
            reverse,
        } = &loaded.revision.state
        else {
            return Ok(());
        };
        let proof = proof.clone();
        let identity = identity.clone();
        let provenance = provenance.clone();
        let reverse = reverse.clone();
        self.verify_undo_binding(loaded)?;
        let current = loaded
            .intent
            .fresh_parent()
            .and_then(|parent| Ok(parent.snapshot()?));
        let complete = if let Some(id) = &reverse {
            if let Ok(mut reversed) = self.load(id) {
                if !self.reverse_matches(loaded, &reversed) {
                    return Err(Error::invalid("Reverse recovery linkage changed"));
                }
                if matches!(reversed.revision.state, State::Undoing { .. }) {
                    return Err(Error::invalid(
                        "Linked reverse requires explicit pending undo recovery",
                    ));
                }
                self.reconcile_loaded(&mut reversed)?;
                matches!(reversed.revision.state, State::Applied { .. })
                    && current
                        .as_ref()
                        .is_ok_and(|current| Self::is_after(&reversed, current))
            } else {
                false
            }
        } else {
            loaded.intent.before.is_none() && matches!(current, Ok(Snapshot::Missing))
        };
        if complete {
            return self.append(
                loaded,
                State::Undone {
                    proof,
                    identity,
                    provenance,
                    reverse,
                },
            );
        }
        if current
            .as_ref()
            .is_ok_and(|current| self.expected_undo(loaded, current).is_ok())
        {
            return Ok(());
        }
        self.append(loaded, State::Conflict { proof: Some(proof) })
    }
    pub(super) fn reverse_matches(&self, original: &Loaded, reverse: &Loaded) -> bool {
        reverse.intent.reverse_of.as_deref()==Some(&original.intent.id)
            && reverse.intent.root==original.intent.root && reverse.intent.path==original.intent.path && reverse.intent.ancestors==original.intent.ancestors
            && reverse.intent.after.matches(&original.before)
            && original.intent.before.as_ref().is_some_and(|before|before.security==reverse.intent.after_security)
            && reverse.intent.before.as_ref().is_some_and(|before|matches!(&original.revision.state,State::Undoing{identity,..}|State::Undone{identity,..} if &before.identity==identity)
                && before.content==original.intent.after && before.security==original.intent.after_security)
    }
    pub(crate) fn reconcile(&self) -> Result<(), Error> {
        self.reconcile_parents()?;
        for id in self.directory.names(super::RECORD_LIMIT * 2 + 1)? {
            if let Ok(mut loaded) = self.load(&id) {
                self.reconcile_loaded(&mut loaded)?;
            }
        }
        Ok(())
    }
    pub(crate) fn undo(&self, id: &str) -> Result<(), Error> {
        self.undo_inner(id, || Ok(()))
    }
    pub(super) fn undo_inner(
        &self,
        id: &str,
        before_unlink: impl FnOnce() -> Result<(), crate::linux_guard::Error>,
    ) -> Result<(), Error> {
        let operation = || -> Result<(), Error> {
            self._lock.revalidate()?;
            let mut loaded = self
                .load(id)
                .map_err(|_| Error::invalid("Recovery record is not verified"))?;
            self.reconcile_loaded(&mut loaded)?;
            if matches!(
                loaded.revision.state,
                State::Undone { .. } | State::NotApplied { .. } | State::Resolved { .. }
            ) {
                return Ok(());
            }
            if !matches!(
                loaded.revision.state,
                State::Applied { .. } | State::Undoing { .. }
            ) {
                return Err(Error::invalid("Recovery state does not permit undo"));
            }
            let parent = loaded.intent.fresh_parent()?;
            let current = parent.snapshot()?;
            let binding = self.expected_undo(&loaded, &current);
            if binding.is_err() {
                let proof = loaded.revision.state.proof().cloned();
                self.append(&mut loaded, State::Conflict { proof })?;
                return Err(Error::invalid("Later destination changes were preserved"));
            }
            let proof = loaded
                .revision
                .state
                .proof()
                .cloned()
                .ok_or_else(|| Error::invalid("Recovery after identity missing"))?;
            let (identity, provenance) = binding?;
            if let Some(before) = &loaded.intent.before {
                let security = before.security.clone();
                if let State::Undoing {
                    reverse: Some(reverse),
                    ..
                } = &loaded.revision.state
                {
                    if self.record_dir(reverse).is_ok() {
                        let reversed = self.load(reverse).map_err(|_| {
                            Error::invalid("Interrupted reverse record is not verified")
                        })?;
                        if !self.reverse_matches(&loaded, &reversed)
                            || !matches!(reversed.revision.state, State::NotApplied { .. })
                        {
                            return Err(Error::invalid(
                                "Interrupted reverse recovery requires reconciliation",
                            ));
                        }
                    }
                }
                let reverse = storage::unique_name("r-")?;
                self.append(
                    &mut loaded,
                    State::Undoing {
                        proof: proof.clone(),
                        identity: identity.clone(),
                        provenance: provenance.clone(),
                        reverse: Some(reverse.clone()),
                    },
                )?;
                self.checkpoint("undoing", id)?;
                let root = Root::reopen(&loaded.intent.root)?;
                let result = self.publish_record(Write {
                    root: &root,
                    parent: &parent,
                    path: &loaded.intent.path,
                    expected: &current,
                    after: &loaded.before,
                    security: &security,
                    id: Some(&reverse),
                    reverse: Some(Reverse {
                        original: &loaded,
                        id: &reverse,
                    }),
                    binding: None,
                });
                result?;
                self.checkpoint("reverseApplied", id)
                    .map_err(|error| error.at(id, true))?;
                self.append(
                    &mut loaded,
                    State::Undone {
                        proof,
                        identity,
                        provenance,
                        reverse: Some(reverse),
                    },
                )
                .map_err(|error| error.at(id, true))?;
            } else {
                if !matches!(loaded.revision.state, State::Undoing { .. }) {
                    self.append(
                        &mut loaded,
                        State::Undoing {
                            proof: proof.clone(),
                            identity: identity.clone(),
                            provenance: provenance.clone(),
                            reverse: None,
                        },
                    )?;
                }
                self.checkpoint("undoing", id)?;
                let authority = || {
                    before_unlink()?;
                    self._lock.revalidate()
                };
                #[cfg(test)]
                let result = parent.remove_authorized_observed(&current, authority, || {
                    self.guard_checkpoint("unlinked", id)
                });
                #[cfg(not(test))]
                let result = parent.remove_authorized(&current, authority);
                if let Err(error) = result {
                    return Err(Error::from(error.error).at(id, error.applied));
                }
                self.checkpoint("unlinkSynced", id)
                    .map_err(|error| error.at(id, true))?;
                self.append(
                    &mut loaded,
                    State::Undone {
                        proof,
                        identity,
                        provenance,
                        reverse: None,
                    },
                )
                .map_err(|error| error.at(id, true))?;
            }
            self.checkpoint("undone", id)
                .map_err(|error| error.at(id, true))
        };
        operation().map_err(|error| {
            let applied = error.applied;
            if error.record.is_some() {
                error
            } else {
                error.at(id, applied)
            }
        })
    }
    pub(crate) fn resolve(&self, id: &str, confirmed: bool) -> Result<(), Error> {
        if !confirmed {
            return Err(Error::invalid(
                "Conflict acknowledgement requires confirmation",
            ));
        }
        let mut loaded = self
            .load(id)
            .map_err(|_| Error::invalid("Recovery record is not verified"))?;
        if matches!(loaded.revision.state, State::Resolved { .. }) {
            return Ok(());
        }
        if !matches!(loaded.revision.state, State::Conflict { .. }) {
            return Err(Error::invalid(
                "Only a verified conflict can be acknowledged",
            ));
        }
        let proof = loaded.revision.state.proof().cloned();
        self.append(&mut loaded, State::Resolved { proof })
    }
}
