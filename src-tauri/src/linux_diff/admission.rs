use super::{
    record::{decode, encode, id_valid, Claim, Manifest, JSON_LIMIT},
    Error, Storage,
};
use crate::linux_guard::{
    root::RootValue,
    storage::{Lock, LockAttempt, PrivateDir, PrivateFile},
    Identity,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(super) struct Policy {
    pub bytes: u64,
    pub allocations: usize,
    pub deadline: Duration,
    pub retry: Duration,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            bytes: 1024 * 1024 * 1024,
            allocations: 1024,
            deadline: Duration::from_secs(5),
            retry: Duration::from_millis(25),
        }
    }
}
struct Waiting {
    deadline: Instant,
    identity: Option<Identity>,
}
impl Waiting {
    fn remaining(&self) -> Result<Duration, Error> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::unavailable("Private diff storage wait expired"));
        }
        Ok(remaining)
    }
    fn observe(&mut self, current: Option<Identity>) -> Result<(), Error> {
        if self
            .identity
            .as_ref()
            .is_some_and(|saved| Some(saved) != current.as_ref())
        {
            return Err(Error::unavailable(
                "Private diff lock identity changed while waiting",
            ));
        }
        if current.is_some() {
            self.identity = current;
        }
        Ok(())
    }
}
fn initial_lock_identity(directory: &PrivateDir) -> Result<Option<Identity>, Error> {
    match directory.file("lock", false) {
        Ok(file) => Ok(Some(file.identity()?)),
        Err(error) if error.kind == crate::linux_guard::ErrorKind::Missing => Ok(None),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn acquire(
    directory: &PrivateDir,
    policy: &Policy,
    cancel: Option<&AtomicBool>,
) -> Result<Lock, Error> {
    let mut waiting = Waiting {
        deadline: Instant::now() + policy.deadline,
        identity: None,
    };
    loop {
        if let Some(cancel) = cancel {
            Error::check(cancel)?;
        }
        waiting.remaining()?;
        directory.revalidate()?;
        waiting.observe(initial_lock_identity(directory)?)?;
        waiting.remaining()?;
        let attempt = directory.try_diff_lock()?;
        waiting.observe(Some(directory.file("lock", false)?.identity()?))?;
        match attempt {
            LockAttempt::Acquired(lock) => {
                if let Some(cancel) = cancel {
                    Error::check(cancel)?;
                }
                waiting.remaining()?;
                return Ok(lock);
            }
            LockAttempt::Busy => {
                #[cfg(test)]
                crate::linux_guard::storage::diff_storage_hook("lock-busy")?;
            }
        }
        let remaining = waiting.remaining()?;
        std::thread::sleep(policy.retry.min(remaining));
    }
}
#[derive(Debug)]
pub(super) struct Inventory {
    pub bytes: u64,
    pub allocations: usize,
}
fn add(total: &mut u64, amount: u64) -> Result<(), Error> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| Error::unavailable("Private diff inventory overflow"))?;
    Ok(())
}
fn artifacts(
    directory: &PrivateDir,
    claim: &Claim,
    manifest: Option<&Manifest>,
) -> Result<u64, Error> {
    let entries = directory.entries(2)?;
    let mut size = directory.native_size()?;
    add(&mut size, 8192)?;
    for entry in entries {
        let planned = claim
            .files
            .iter()
            .find(|planned| planned.name == entry.name)
            .ok_or_else(|| Error::unavailable("Unknown private diff allocation artifact"))?;
        if entry.directory || entry.size > planned.length {
            return Err(Error::unavailable("Unsafe private diff partial artifact"));
        }
        if let Some(manifest) = manifest {
            let proof = manifest
                .files
                .iter()
                .find(|file| file.name == entry.name)
                .ok_or_else(|| Error::unavailable("Missing private diff manifest file"))?;
            if proof.identity != entry.identity || proof.length != entry.size {
                return Err(Error::unavailable(
                    "Private diff ready artifact identity changed",
                ));
            }
        }
        add(&mut size, entry.size)?;
    }
    Ok(size)
}
pub(super) fn inventory(
    directory: &PrivateDir,
    lock: &Lock,
    policy: &Policy,
) -> Result<Inventory, Error> {
    lock.revalidate()?;
    let entries = directory.entries(3073)?;
    let entries = entries
        .into_iter()
        .map(|entry| (entry.name.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::from(["lock".to_string()]);
    let lock_entry = entries
        .get("lock")
        .ok_or_else(|| Error::unavailable("Private diff inventory lock missing"))?;
    if lock_entry.directory {
        return Err(Error::unavailable("Invalid private diff lock artifact"));
    }
    directory.diff_filesystem()?;
    if directory.native_size()? > crate::linux_guard::storage::DIFF_NAMESPACE_BYTES {
        return Err(Error::unavailable("Private diff namespace exceeds its allowance").retain());
    }
    let mut bytes = crate::linux_guard::storage::DIFF_NAMESPACE_BYTES;
    add(&mut bytes, 4096)?;
    add(&mut bytes, lock_entry.size)?;
    let mut allocations = 0usize;
    for (name, entry) in &entries {
        let Some(id) = name
            .strip_prefix("reservation-")
            .and_then(|name| name.strip_suffix(".json"))
        else {
            continue;
        };
        if !id_valid(id) || entry.directory || entry.size > JSON_LIMIT as u64 {
            return Err(Error::unavailable("Invalid private diff claim artifact"));
        }
        allocations = allocations
            .checked_add(1)
            .ok_or_else(|| Error::unavailable("Private diff allocation count overflow"))?;
        let claim_bytes = directory.file(name, false)?.read(JSON_LIMIT)?;
        let claim: Claim = decode(&claim_bytes)?;
        claim.validate()?;
        if claim.directory_name != id || claim.namespace_identity != directory.identity()? {
            return Err(Error::unavailable("Private diff claim namespace changed"));
        }
        seen.insert(name.clone());
        let manifest_name = format!("allocation-{id}.json");
        let mut native = entry.size;
        let manifest = if let Some(entry) = entries.get(&manifest_name) {
            if entry.directory || entry.size > JSON_LIMIT as u64 {
                return Err(Error::unavailable("Invalid private diff manifest artifact"));
            }
            let manifest: Manifest =
                decode(&directory.file(&manifest_name, false)?.read(JSON_LIMIT)?)?;
            manifest.validate(&claim, &claim_bytes)?;
            seen.insert(manifest_name);
            add(&mut native, entry.size)?;
            Some(manifest)
        } else {
            None
        };
        if let Some(entry) = entries.get(id) {
            if !entry.directory
                || manifest
                    .as_ref()
                    .is_some_and(|manifest| manifest.directory_identity != entry.identity)
            {
                return Err(Error::unavailable(
                    "Private diff directory identity changed",
                ));
            }
            add(
                &mut native,
                artifacts(&directory.lookup(id)?, &claim, manifest.as_ref())?,
            )?;
            seen.insert(id.to_string());
        }
        if native > claim.reserved_bytes {
            return Err(
                Error::unavailable("Private diff native overhead exceeds the reservation").retain(),
            );
        }
        add(&mut bytes, claim.reserved_bytes)?;
    }
    if entries.keys().any(|name| !seen.contains(name)) {
        return Err(Error::unavailable(
            "Unknown private diff namespace artifact",
        ));
    }
    if allocations > policy.allocations || bytes > policy.bytes {
        return Err(Error::unavailable(
            "Private diff storage quota is exhausted",
        ));
    }
    lock.revalidate()?;
    Ok(Inventory { bytes, allocations })
}
pub(super) struct Reservation {
    pub namespace: PrivateDir,
    pub claim: Claim,
    pub bytes: Vec<u8>,
    pub file: PrivateFile,
}
impl Storage {
    pub(super) fn reserve(
        &self,
        roots: &[RootValue],
        cancel: &AtomicBool,
        bytes: &[Vec<u8>; 2],
    ) -> Result<Reservation, Error> {
        Error::check(cancel)?;
        let namespace = self.initialize(roots)?;
        let lock = acquire(&namespace, &self.policy, Some(cancel))?;
        let usage = inventory(&namespace, &lock, &self.policy)?;
        let claim = Claim::new(
            namespace.identity()?,
            crate::linux_guard::storage::unique_name("d-")?,
            bytes,
        )?;
        if usage.allocations >= self.policy.allocations
            || usage
                .bytes
                .checked_add(claim.reserved_bytes)
                .is_none_or(|value| value > self.policy.bytes)
        {
            return Err(Error::unavailable(
                "Private diff storage quota is exhausted",
            ));
        }
        self.initialize(roots)?.identity().and_then(|current| {
            if current == claim.namespace_identity {
                Ok(current)
            } else {
                Err(crate::linux_guard::Error::conflict(
                    "Private diff namespace changed",
                ))
            }
        })?;
        lock.revalidate()?;
        Error::check(cancel)?;
        let encoded = encode(&claim)?;
        let before = namespace.before_diff_entry()?;
        let created = namespace.file(&format!("reservation-{}.json", claim.directory_name), true);
        namespace
            .after_diff_entry(before)
            .map_err(|error| Error::from(error).retain())?;
        let file = created?;
        file.write(&encoded)?;
        if file.read(JSON_LIMIT)? != encoded {
            return Err(Error::unavailable("Private diff claim verification failed"));
        }
        namespace.sync()?;
        lock.revalidate()?;
        Ok(Reservation {
            namespace,
            claim,
            bytes: encoded,
            file,
        })
    }
}
