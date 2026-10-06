use super::{
    binding::guard_error,
    inventory::verify_cleanup_directory,
    record::{self, DirectorySecurity, ParentCleanup, LIMIT},
    Error, Journal, Loaded,
};
use crate::linux_guard::storage::{ArtifactRemoval, DirectoryRemoval, FileProof, PrivateDir};
use crate::linux_journal::cleanup::CleanupResult;
fn name(id: &str) -> String {
    format!("parent-cleanup-{id}.json")
}
impl Journal {
    fn parent_cleanup_proof(&self, loaded: &Loaded) -> Result<ParentCleanup, Error> {
        if !loaded.revision.state.cleanable() {
            return Err(Error::invalid("Parent state is not eligible for cleanup"));
        }
        let names = loaded
            .directory
            .names(loaded.intent.revisions() as usize + 1)?;
        let expected = std::iter::once("intent.json".to_string())
            .chain((1..=loaded.revision.sequence).map(crate::linux_journal::state::name))
            .collect::<Vec<_>>();
        if names != expected {
            return Err(Error::invalid("Unknown parent cleanup artifacts retained"));
        }
        let artifacts = names
            .iter()
            .map(|name| loaded.directory.file(name, false)?.proof(LIMIT))
            .collect::<Result<Vec<_>, crate::linux_guard::Error>>()?;
        let (uid, gid, mode) = loaded.directory.security()?;
        let proof = ParentCleanup {
            id: loaded.intent.id.clone(),
            directory_identity: loaded.directory.identity()?,
            directory_security: DirectorySecurity { uid, gid, mode },
            intent: loaded.intent.clone(),
            intent_sha256: crate::linux_journal::record::hash(&loaded.intent_bytes),
            revision: loaded.revision.clone(),
            revision_fingerprint: loaded.revision.fingerprint()?,
            artifacts,
        };
        proof.validate()?;
        let bytes = record::encode(&proof)?;
        let actual = proof
            .artifacts
            .iter()
            .try_fold(loaded.directory.native_size()?, |sum, file| {
                sum.checked_add(file.length)
                    .ok_or_else(|| Error::invalid("Parent cleanup size overflow"))
            })?
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| Error::invalid("Parent cleanup size overflow"))?;
        if actual > proof.intent.reserved_bytes || loaded.directory.native_size()? > 4096 {
            return Err(Error::invalid(
                "Parent cleanup native artifacts exceed the reservation",
            ));
        }
        verify_cleanup_directory(&proof, &loaded.directory)?;
        Ok(proof)
    }
    pub(super) fn verify_parent_cleanup_layout(
        &self,
        proof: &ParentCleanup,
    ) -> Result<Option<PrivateDir>, Error> {
        self._lock.revalidate()?;
        let names = self
            .directory
            .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)?;
        if !names.contains(&proof.id) {
            return Ok(None);
        }
        let directory = self.directory.lookup(&proof.id)?;
        verify_cleanup_directory(proof, &directory)?;
        if directory
            .names(proof.artifacts.len())?
            .contains(&"intent.json".to_string())
        {
            let bytes = directory.file("intent.json", false)?.read(LIMIT)?;
            if bytes != record::encode(&proof.intent)? {
                return Err(Error::invalid("Parent cleanup and record intents differ"));
            }
        }
        Ok(Some(directory))
    }
    fn verify_parent_cleanup(
        &self,
        proof: &ParentCleanup,
        artifact: &FileProof,
    ) -> Result<Option<PrivateDir>, Error> {
        let file = self.directory.file(&artifact.name, false)?;
        if file.proof(LIMIT)? != *artifact {
            return Err(Error::invalid("Parent cleanup proof authority changed"));
        }
        let current: ParentCleanup = record::decode(&file.read(LIMIT)?)
            .map_err(|_| Error::invalid("Parent cleanup proof is not verified"))?;
        current.validate()?;
        if current != *proof {
            return Err(Error::invalid("Parent cleanup proof content changed"));
        }
        let directory = self.verify_parent_cleanup_layout(proof)?;
        self._lock.revalidate()?;
        Ok(directory)
    }
    fn publish_parent_cleanup(&self, loaded: &Loaded) -> Result<(), Error> {
        let proof = self.parent_cleanup_proof(loaded)?;
        let bytes = record::encode(&proof)?;
        let fresh = self
            .load_parent(&loaded.intent.id)
            .map_err(|_| Error::invalid("Parent cleanup record changed"))?;
        if fresh.intent_bytes != loaded.intent_bytes
            || fresh.revision.fingerprint()? != proof.revision_fingerprint
        {
            return Err(Error::invalid("Parent cleanup revision authority changed"));
        }
        verify_cleanup_directory(&proof, &loaded.directory)?;
        self._lock.revalidate()?;
        let file = self.directory.file(&name(&proof.id), true)?;
        self.parent_checkpoint("parentCleanupProofCreated", &proof.id)?;
        file.write(&bytes)?;
        self.parent_checkpoint("parentCleanupProofWritten", &proof.id)?;
        if file.read(LIMIT)? != bytes {
            return Err(Error::invalid("Parent cleanup proof verification failed"));
        }
        self.parent_checkpoint("parentCleanupProofVerified", &proof.id)?;
        file.sync()?;
        self.parent_checkpoint("parentCleanupProofSynced", &proof.id)?;
        self.directory.sync()?;
        self.parent_checkpoint("parentCleanupPrepared", &proof.id)
    }
    fn remove_parent_artifacts(
        &self,
        proof: &ParentCleanup,
        artifact: &FileProof,
        removed: &mut u32,
    ) -> Result<(), Error> {
        for expected in &proof.artifacts {
            let Some(directory) = self.verify_parent_cleanup(proof, artifact)? else {
                return Ok(());
            };
            if !directory
                .names(proof.artifacts.len())?
                .contains(&expected.name)
            {
                continue;
            }
            let result = directory.unlink_authorized(ArtifactRemoval {
                proof: expected,
                lock: &self._lock,
                limit: LIMIT,
                binding: || {
                    self.verify_parent_cleanup(proof, artifact)
                        .map(|_| ())
                        .map_err(guard_error)
                },
                after_unlink: || {
                    self.parent_checkpoint("parentCleanupArtifactUnlinked", &proof.id)
                        .map_err(guard_error)
                },
            });
            if result.as_ref().map_or_else(|error| error.applied, |_| true) {
                *removed += 1;
            }
            result.map_err(|error| Error::from(error.error))?;
            self.parent_checkpoint("parentCleanupArtifact", &proof.id)?;
        }
        Ok(())
    }
    fn resume_parent_cleanup(
        &self,
        proof: &ParentCleanup,
        artifact: &FileProof,
        removed: &mut u32,
    ) -> Result<(), Error> {
        self.remove_parent_artifacts(proof, artifact, removed)?;
        if self.verify_parent_cleanup(proof, artifact)?.is_some() {
            self.parent_checkpoint("parentCleanupEmpty", &proof.id)?;
            self.directory
                .remove_empty_authorized(
                    &proof.id,
                    DirectoryRemoval {
                        identity: &proof.directory_identity,
                        lock: &self._lock,
                        binding: || {
                            self.verify_parent_cleanup(proof, artifact)
                                .map(|_| ())
                                .map_err(guard_error)
                        },
                        after_unlink: || {
                            self.parent_checkpoint("parentCleanupDirectoryUnlinked", &proof.id)
                                .map_err(guard_error)
                        },
                    },
                )
                .map_err(|error| Error::from(error.error))?;
            self.parent_checkpoint("parentCleanupDirectory", &proof.id)?;
        }
        self.verify_parent_cleanup(proof, artifact)?;
        self.directory
            .unlink_authorized(ArtifactRemoval {
                proof: artifact,
                lock: &self._lock,
                limit: LIMIT,
                binding: || {
                    self.verify_parent_cleanup(proof, artifact)
                        .map(|_| ())
                        .map_err(guard_error)
                },
                after_unlink: || {
                    self.parent_checkpoint("parentCleanupProofUnlinked", &proof.id)
                        .map_err(guard_error)
                },
            })
            .map_err(|error| Error::from(error.error))?;
        self.parent_checkpoint("parentCleanupComplete", &proof.id)
    }
    pub(crate) fn cleanup_parent(&self, id: &str, confirmed: bool) -> Result<CleanupResult, Error> {
        if !confirmed {
            return Err(Error::invalid("Parent cleanup requires confirmation"));
        }
        if !record::id_valid(id) {
            return Err(Error::invalid("Invalid parent record identity"));
        }
        self._lock.revalidate()?;
        let names = self
            .directory
            .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)?;
        let proof_name = name(id);
        if !names.contains(&id.to_string()) && !names.contains(&proof_name) {
            return Ok(CleanupResult {
                removed: 0,
                complete: true,
                warning: None,
            });
        }
        if !names.contains(&proof_name) {
            let loaded = self
                .load_parent(id)
                .map_err(|_| Error::invalid("Parent record is not verified"))?;
            self.publish_parent_cleanup(&loaded)?;
        }
        let file = self.directory.file(&proof_name, false)?;
        let bytes = file.read(LIMIT)?;
        let artifact = file.proof(LIMIT)?;
        let proof: ParentCleanup = record::decode(&bytes)
            .map_err(|_| Error::invalid("Parent cleanup proof is not verified"))?;
        proof.validate()?;
        if proof.id != id {
            return Err(Error::invalid("Parent cleanup identity differs"));
        }
        let mut removed = 0;
        let result = self.resume_parent_cleanup(&proof, &artifact, &mut removed);
        match result {
            Ok(()) => Ok(CleanupResult {
                removed,
                complete: true,
                warning: None,
            }),
            Err(error) => Ok(CleanupResult {
                removed,
                complete: false,
                warning: Some(error.message),
            }),
        }
    }
}
