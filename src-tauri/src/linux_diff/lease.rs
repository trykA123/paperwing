use super::{
    admission::{acquire, inventory, Reservation},
    record::{encode, hash, Manifest, JSON_LIMIT},
    Error, Storage,
};
use crate::linux_guard::{
    storage::{FileProof, PrivateDir, PrivateFile},
    BytePermit, FILE_LIMIT,
};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use tokio::sync::OwnedSemaphorePermit;

struct Owner {
    storage: Arc<Storage>,
    reservation: Reservation,
    directory: Option<PrivateDir>,
    files: Vec<FileProof>,
    manifest: Option<(PrivateFile, Vec<u8>)>,
}
impl Owner {
    fn validate_sources(&self, roots: &[crate::linux_guard::root::RootValue]) -> Result<(), Error> {
        if self.storage.initialize(roots)?.identity()? != self.reservation.claim.namespace_identity
        {
            return Err(Error::unavailable(
                "Private diff source or namespace authority changed",
            ));
        }
        Ok(())
    }
    fn directory(&self) -> Result<&PrivateDir, Error> {
        self.directory
            .as_ref()
            .ok_or_else(|| Error::unavailable("Private diff directory owner missing"))
    }
    fn materialize(
        &mut self,
        roots: &[crate::linux_guard::root::RootValue],
        cancel: &AtomicBool,
        bytes: &[Vec<u8>; 2],
    ) -> Result<PathBuf, Error> {
        Error::check(cancel)?;
        #[cfg(test)]
        crate::linux_guard::storage::diff_storage_hook("before-directory")?;
        self.validate_sources(roots)?;
        self.create_directory(roots, cancel)?;
        #[cfg(test)]
        crate::linux_guard::storage::diff_storage_hook("directory-created")?;
        self.write_files(roots, cancel, bytes)?;
        self.publish(roots, cancel)?;
        Ok(self.directory()?.path().to_path_buf())
    }
    fn create_directory(
        &mut self,
        roots: &[crate::linux_guard::root::RootValue],
        cancel: &AtomicBool,
    ) -> Result<(), Error> {
        let namespace = &self.reservation.namespace;
        let lock = acquire(namespace, &self.storage.policy, Some(cancel))?;
        self.validate_sources(roots)?;
        if self.reservation.file.read(JSON_LIMIT)? != self.reservation.bytes {
            return Err(Error::unavailable("Private diff reservation changed"));
        }
        inventory(namespace, &lock, &self.storage.policy)?;
        Error::check(cancel)?;
        lock.revalidate()?;
        let before = namespace
            .before_diff_entry()
            .map_err(|error| Error::from(error).retain())?;
        let created = namespace.create_child(&self.reservation.claim.directory_name);
        if let Ok(directory) = &created {
            self.directory = Some(directory.clone());
        }
        namespace
            .after_diff_entry(before)
            .map_err(|error| Error::from(error).retain())?;
        created.map_err(|error| Error::from(error).retain())?;
        lock.revalidate()?;
        Ok(())
    }
    fn write_files(
        &mut self,
        roots: &[crate::linux_guard::root::RootValue],
        cancel: &AtomicBool,
        bytes: &[Vec<u8>; 2],
    ) -> Result<(), Error> {
        let directory = self.directory()?.clone();
        for (planned, bytes) in self.reservation.claim.files.iter().zip(bytes) {
            Error::check(cancel)?;
            self.validate_sources(roots)?;
            let file = directory.file(&planned.name, true)?;
            let mut proof = file.proof(FILE_LIMIT)?;
            proof.length = planned.length;
            proof.sha256 = planned.sha256.clone();
            self.files.push(proof.clone());
            file.write(bytes)?;
            if file.proof(FILE_LIMIT)? != proof {
                return Err(Error::unavailable(
                    "Private diff materialization verification failed",
                ));
            }
            directory.sync()?;
        }
        Ok(())
    }
    fn publish(
        &mut self,
        roots: &[crate::linux_guard::root::RootValue],
        cancel: &AtomicBool,
    ) -> Result<(), Error> {
        let namespace = &self.reservation.namespace;
        let lock = acquire(namespace, &self.storage.policy, Some(cancel))?;
        self.validate_sources(roots)?;
        if self.reservation.file.read(JSON_LIMIT)? != self.reservation.bytes {
            return Err(Error::unavailable("Private diff reservation changed"));
        }
        let manifest = Manifest {
            claim_sha256: hash(&self.reservation.bytes),
            directory_identity: self.directory()?.identity()?,
            files: self.files.clone(),
        };
        manifest.validate(&self.reservation.claim, &self.reservation.bytes)?;
        let encoded = encode(&manifest)?;
        lock.revalidate()?;
        Error::check(cancel)?;
        let before = namespace
            .before_diff_entry()
            .map_err(|error| Error::from(error).retain())?;
        let created = namespace.file(
            &format!("allocation-{}.json", self.reservation.claim.directory_name),
            true,
        );
        namespace
            .after_diff_entry(before)
            .map_err(|error| Error::from(error).retain())?;
        let file = created.map_err(|error| Error::from(error).retain())?;
        self.manifest = Some((file, encoded));
        #[cfg(test)]
        crate::linux_guard::storage::diff_storage_hook("manifest-created")?;
        let (file, encoded) = self
            .manifest
            .as_ref()
            .ok_or_else(|| Error::unavailable("Private diff manifest owner missing"))?;
        file.write(encoded)?;
        if file.read(JSON_LIMIT)? != *encoded {
            return Err(Error::unavailable(
                "Private diff manifest verification failed",
            ));
        }
        namespace.sync()?;
        lock.revalidate()?;
        inventory(namespace, &lock, &self.storage.policy)?;
        Error::check(cancel)?;
        Ok(())
    }
    fn record_proofs(&self) -> Result<(FileProof, Option<FileProof>), Error> {
        let claim_proof = self.reservation.file.proof(JSON_LIMIT)?;
        if self.reservation.file.read(JSON_LIMIT)? != self.reservation.bytes {
            return Err(Error::unavailable("Private diff cleanup claim changed"));
        }
        let manifest_proof = if let Some((file, expected)) = &self.manifest {
            if file.read(JSON_LIMIT)? != *expected {
                return Err(Error::unavailable("Private diff cleanup manifest changed"));
            }
            Some(file.proof(JSON_LIMIT)?)
        } else {
            None
        };
        Ok((claim_proof, manifest_proof))
    }
    fn cleanup_directory(&self, lock: &crate::linux_guard::storage::Lock) -> Result<(), Error> {
        let Some(directory) = &self.directory else {
            return Ok(());
        };
        let identity = directory.identity()?;
        let entries = directory.entries(2)?;
        if entries.len() != self.files.len()
            || entries.iter().any(|entry| {
                !self.files.iter().any(|file| {
                    file.name == entry.name
                        && file.identity == entry.identity
                        && file.length == entry.size
                })
            })
        {
            return Err(Error::unavailable(
                "Private diff cleanup allocation changed",
            ));
        }
        for proof in &self.files {
            if directory.file(&proof.name, false)?.proof(FILE_LIMIT)? != *proof {
                return Err(Error::unavailable("Private diff cleanup content changed"));
            }
        }
        for proof in &self.files {
            directory.unlink_proven(proof, lock)?;
        }
        lock.revalidate()?;
        self.reservation.namespace.remove_empty_locked(
            &self.reservation.claim.directory_name,
            &identity,
            lock,
        )?;
        Ok(())
    }
    fn cleanup(self) -> Result<(), Error> {
        let namespace = &self.reservation.namespace;
        let lock = acquire(namespace, &self.storage.policy, None)?;
        let (claim_proof, manifest_proof) = self.record_proofs()?;
        self.cleanup_directory(&lock)?;
        if let Some(proof) = manifest_proof {
            #[cfg(test)]
            crate::linux_guard::storage::diff_storage_hook("manifest-removal")?;
            namespace.unlink_proven(&proof, &lock)?;
        }
        #[cfg(test)]
        crate::linux_guard::storage::diff_storage_hook("before-claim-removal")?;
        namespace.unlink_proven(&claim_proof, &lock)?;
        Ok(())
    }
}

struct Cleanup {
    owner: Owner,
    _work: OwnedSemaphorePermit,
    _memory: BytePermit,
}
impl Cleanup {
    fn run(self) -> Result<(), Error> {
        #[cfg(test)]
        let _hook = self.owner.storage.install_hook();
        let Self {
            owner,
            _work,
            _memory,
        } = self;
        let result = owner.cleanup();
        drop(_work);
        drop(_memory);
        result
    }
}
pub(crate) struct Lease {
    path: PathBuf,
    runtime: tokio::runtime::Handle,
    cleanup: Option<Cleanup>,
}
impl Lease {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) async fn finish(mut self) -> Result<(), Error> {
        let cleanup = self
            .cleanup
            .take()
            .ok_or_else(|| Error::unavailable("Private diff cleanup owner missing"))?;
        self.runtime
            .spawn_blocking(move || cleanup.run())
            .await
            .map_err(|_| Error::unavailable("Private diff cleanup work unavailable"))?
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            std::mem::drop(self.runtime.spawn_blocking(move || cleanup.run()));
        }
    }
}
impl Storage {
    pub(crate) async fn materialize(
        self: &Arc<Self>,
        roots: Vec<crate::linux_guard::root::RootValue>,
        cancel: Arc<AtomicBool>,
        inputs: [&[u8]; 2],
    ) -> Result<Lease, Error> {
        let [left, right] = inputs;
        if left.len() > FILE_LIMIT || right.len() > FILE_LIMIT {
            return Err(Error::unavailable("Private diff input exceeds the limit"));
        }
        let work = self.permit(&cancel).await?;
        let size = left
            .len()
            .checked_add(right.len())
            .ok_or_else(|| Error::unavailable("Private diff memory accounting overflow"))?;
        let memory = BytePermit::acquire(size)?;
        Error::check(&cancel)?;
        let bytes = [left.to_vec(), right.to_vec()];
        let storage = self.clone();
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::unavailable("Private diff cleanup runtime unavailable"))?;
        tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            let _hook = storage.install_hook();
            #[cfg(test)]
            crate::linux_guard::storage::diff_storage_hook("storage-start")?;
            let reservation = storage.reserve(&roots, &cancel, &bytes)?;
            let mut cleanup = Cleanup {
                owner: Owner {
                    storage,
                    reservation,
                    directory: None,
                    files: Vec::new(),
                    manifest: None,
                },
                _work: work,
                _memory: memory,
            };
            match cleanup.owner.materialize(&roots, &cancel, &bytes) {
                Ok(path) => Ok(Lease {
                    path,
                    runtime,
                    cleanup: Some(cleanup),
                }),
                Err(error) => {
                    if !error.retained {
                        let _ = cleanup.run();
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| Error::unavailable("Private diff materialization work unavailable"))?
    }
}
