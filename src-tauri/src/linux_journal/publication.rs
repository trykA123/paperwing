use super::{
    admission::Admission,
    parents::binding::{FinalPublication, ParentBinding},
};
use super::{
    encode, hash,
    record::{Before, Content, Intent},
    state::{self, Revision, State},
    Error, Journal, Loaded,
};
use crate::linux_guard::{
    metadata::Security,
    mutation::{AuthorizedPublication, OperationAuthority, Parent, Snapshot},
    storage, Identity, Root,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub(crate) struct Published {
    pub record: String,
    pub identity: Identity,
    pub security: Security,
}
pub(super) struct Write<'a> {
    pub root: &'a Root,
    pub parent: &'a Parent,
    pub path: &'a str,
    pub expected: &'a Snapshot,
    pub after: &'a [u8],
    pub security: &'a Security,
    pub id: Option<&'a str>,
    pub reverse: Option<Reverse<'a>>,
    pub binding: Option<&'a ParentBinding>,
}
pub(super) struct Reverse<'a> {
    pub original: &'a Loaded,
    pub id: &'a str,
}
impl Journal {
    pub(crate) fn replace(
        &self,
        root: &Root,
        path: &str,
        expected: &Snapshot,
        after: &[u8],
    ) -> Result<Published, Error> {
        #[cfg(test)]
        if !self.writer {
            return Err(Error::invalid("New Linux writer disabled"));
        }
        let parent = root.parent(path, false)?;
        let security = match expected {
            Snapshot::Missing => Security::new_file(),
            Snapshot::Regular { security, .. } => security.clone(),
        };
        self.publish_record(Write {
            root,
            parent: &parent,
            path,
            expected,
            after,
            security: &security,
            id: None,
            reverse: None,
            binding: None,
        })
    }
    pub(super) fn publish_record(&self, write: Write<'_>) -> Result<Published, Error> {
        self.publish_record_inner(write, None)
    }
    pub(super) fn publish_record_authorized(
        &self,
        write: Write<'_>,
        caller: &mut dyn OperationAuthority,
    ) -> Result<Published, Error> {
        self.publish_record_inner(write, Some(caller))
    }
    fn publish_record_inner(
        &self,
        write: Write<'_>,
        mut caller: Option<&mut dyn OperationAuthority>,
    ) -> Result<Published, Error> {
        let Write {
            root,
            parent,
            path,
            expected,
            after,
            security,
            id,
            reverse,
            binding,
        } = write;
        if binding.is_some() && caller.is_none() {
            return Err(Error::invalid(
                "Bound publication caller authority is missing",
            ));
        }
        if after.len() > crate::linux_guard::FILE_LIMIT {
            return Err(Error::invalid("Linux recovery file limit reached"));
        }
        security.validate()?;
        parent.validate(expected)?;
        let id = id
            .map(str::to_string)
            .map(Ok)
            .unwrap_or_else(|| storage::unique_name("r-"))?;
        let candidate = storage::unique_name(".paperwing-stage-")?;
        let before = expected.bytes().unwrap_or_default();
        let intent = Intent {
            id: id.clone(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::invalid("Recovery clock unavailable"))?
                .as_millis(),
            root: root.value()?,
            path: path.into(),
            ancestors: parent.ancestors()?,
            before: match expected {
                Snapshot::Missing => None,
                Snapshot::Regular {
                    identity, security, ..
                } => Some(Before {
                    identity: identity.clone(),
                    content: Content::of(before),
                    security: security.clone(),
                }),
            },
            after: Content::of(after),
            after_security: security.clone(),
            reverse_of: reverse
                .as_ref()
                .map(|reverse| reverse.original.intent.id.clone()),
        };
        intent.validate()?;
        if caller.is_some() {
            self.parent_authority(&intent.root)?;
        }
        if let Some(binding) = binding {
            self.verify_parent_write(binding, &intent)?;
        }
        self.directory.outside_root(&intent.root.identity)?;
        root.revalidate()?;
        let intent_bytes = encode(&intent)?;
        let metadata_size = security
            .attributes
            .iter()
            .try_fold(0u64, |total, (name, bytes)| {
                total.checked_add((name.len() + bytes.len()) as u64)
            })
            .ok_or_else(|| Error::invalid("Stage metadata reservation overflow"))?;
        let reservation = (before.len() as u64)
            .checked_add(after.len() as u64 * 2)
            .and_then(|n| n.checked_add(intent_bytes.len() as u64 + metadata_size + 128 * 1024))
            .ok_or_else(|| Error::invalid("Recovery reservation overflow"))?;
        self.admit(
            parent,
            Admission {
                reverse: reverse.as_ref(),
                binding,
                reservation,
            },
        )?;
        let operation = || -> Result<Published, Error> {
            if let Some(caller) = caller.as_deref_mut() {
                caller.refresh()?;
                self.parent_authority(&intent.root)?;
                parent.validate(expected)?;
                if let Some(binding) = binding {
                    self.verify_parent_write(binding, &intent)?;
                }
                self._lock.revalidate()?;
                caller.check()?;
            }
            let directory = self.directory.create_child(&id)?;
            directory.write_new("before", before)?;
            self.checkpoint("firstBackup", &id)?;
            directory.write_new("after", after)?;
            self.checkpoint("backups", &id)?;
            directory.write_new("intent.json", &intent_bytes)?;
            self.checkpoint("intent", &id)?;
            let revision = Revision {
                sequence: 1,
                previous: None,
                intent: hash(&intent_bytes),
                state: State::Prepared {
                    candidate: candidate.clone(),
                },
            };
            directory.write_new(&state::name(1), &encode(&revision)?)?;
            self.directory.sync()?;
            let mut loaded = Loaded {
                directory,
                intent,
                intent_hash: hash(&intent_bytes),
                revision,
                before: before.to_vec(),
                after: after.to_vec(),
            };
            self.checkpoint("prepared", &id)?;
            let stage = parent.stage_named(after, expected, &candidate)?;
            self.checkpoint("candidate", &id)?;
            parent.prepare_security(&stage, security)?;
            let proof = stage.proof()?;
            parent.verify_stage(&proof, after, security)?;
            self.append(
                &mut loaded,
                State::Staged {
                    proof: proof.clone(),
                },
            )?;
            self.checkpoint("staged", &id)?;
            self.append(
                &mut loaded,
                State::Replacing {
                    proof: proof.clone(),
                },
            )?;
            self.checkpoint("replacing", &id)?;
            let result = if let Some(caller) = caller {
                parent.publish_authorized(
                    stage,
                    AuthorizedPublication {
                        expected,
                        security,
                        caller,
                        binding: || {
                            self.verify_final_publication(FinalPublication {
                                binding,
                                intent: &loaded.intent,
                                intent_hash: &loaded.intent_hash,
                                revision: &loaded.revision,
                                parent,
                                expected,
                                proof: &proof,
                            })
                        },
                        after_mutation: || {
                            #[cfg(test)]
                            return self.guard_checkpoint("renamed", &id);
                            #[cfg(not(test))]
                            Ok(())
                        },
                    },
                )
            } else {
                #[cfg(test)]
                {
                    parent.publish_observed(stage, expected, security, || {
                        self.guard_checkpoint("renamed", &id)
                    })
                }
                #[cfg(not(test))]
                {
                    parent.publish_restore(stage, expected, security)
                }
            };
            let published = match result {
                Ok(published) => published,
                Err(error) => return Err(Error::from(error.error).at(&id, error.applied)),
            };
            self.checkpoint("directorySynced", &id)
                .map_err(|error| error.at(&id, true))?;
            self.append(
                &mut loaded,
                State::Applied {
                    proof,
                    identity: published.identity.clone(),
                },
            )
            .map_err(|error| error.at(&id, true))?;
            self.checkpoint("applied", &id)
                .map_err(|error| error.at(&id, true))?;
            Ok(Published {
                record: id.clone(),
                identity: published.identity,
                security: security.clone(),
            })
        };
        operation().map_err(|error| {
            let applied = error.applied;
            error.at(&id, applied)
        })
    }
    pub(super) fn checkpoint(&self, phase: &str, id: &str) -> Result<(), Error> {
        #[cfg(test)]
        {
            self.parent_checkpoint(&format!("file{phase}"), id)?;
            if let Ok(target) = crate::env_names::var("SKEIN_JOURNAL_KILL_PHASE") {
                if target == phase {
                    super::tests::persist_checkpoint(phase, id);
                    loop {
                        std::thread::park();
                    }
                }
            }
            if self.fault == Some(phase) {
                return Err(
                    Error::invalid("Injected native journal checkpoint failure").at(id, false)
                );
            }
        }
        let _ = (phase, id);
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn guard_checkpoint(
        &self,
        phase: &str,
        id: &str,
    ) -> Result<(), crate::linux_guard::Error> {
        self.checkpoint(phase, id)
            .map_err(|_| crate::linux_guard::Error {
                kind: crate::linux_guard::ErrorKind::Io,
                message: "Injected failure after native mutation",
                code: Some(libc::EIO),
            })
    }
}
