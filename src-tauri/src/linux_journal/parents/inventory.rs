use super::binding::ParentBinding;
use super::{
    record::{decode, encode, id_valid, ParentCleanup, ParentIntent, LIMIT},
    Error, Journal,
};
use crate::linux_guard::storage::{Entry, PrivateDir};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::linux_journal) struct Usage {
    pub bytes: u64,
    pub records: usize,
    pub blocked: bool,
    pub names: BTreeSet<String>,
}
fn add(total: &mut u64, bytes: u64) -> Result<(), Error> {
    *total = total
        .checked_add(bytes)
        .ok_or_else(|| Error::invalid("Parent inventory size overflow"))?;
    Ok(())
}
fn actual(directory: Option<&PrivateDir>, proof_bytes: u64) -> Result<u64, Error> {
    let mut size = proof_bytes;
    if let Some(directory) = directory {
        let native = directory.native_size()?;
        if native > 4096 {
            return Err(Error::invalid(
                "Parent record directory exceeds its reserved allowance",
            ));
        }
        add(&mut size, native)?;
        for entry in directory.entries(crate::linux_journal::ARTIFACT_LIMIT)? {
            if entry.directory {
                return Err(Error::invalid(
                    "Nested parent record artifacts are unsupported",
                ));
            }
            add(&mut size, entry.size)?;
        }
    }
    Ok(size)
}
pub(super) fn verify_cleanup_directory(
    proof: &ParentCleanup,
    directory: &PrivateDir,
) -> Result<(), Error> {
    if directory.identity()? != proof.directory_identity
        || directory.security()?
            != (
                proof.directory_security.uid,
                proof.directory_security.gid,
                proof.directory_security.mode,
            )
    {
        return Err(Error::invalid("Parent cleanup directory authority changed"));
    }
    for entry in directory.entries(proof.artifacts.len())? {
        let expected = proof
            .artifacts
            .iter()
            .find(|artifact| artifact.name == entry.name)
            .ok_or_else(|| Error::invalid("Unknown parent cleanup artifact retained"))?;
        if entry.directory || directory.file(&entry.name, false)?.proof(LIMIT)? != *expected {
            return Err(Error::invalid("Parent cleanup artifact authority changed"));
        }
    }
    Ok(())
}
impl Journal {
    pub(in crate::linux_journal) fn parent_usage(&self) -> Result<Usage, Error> {
        self.parent_usage_bound(None)
    }
    pub(in crate::linux_journal) fn parent_usage_bound(
        &self,
        binding: Option<&ParentBinding>,
    ) -> Result<Usage, Error> {
        self._lock.revalidate()?;
        let entries = self
            .directory
            .entries(crate::linux_journal::RECORD_LIMIT * 2 + 1)?
            .into_iter()
            .map(|entry| (entry.name.clone(), entry))
            .collect::<BTreeMap<_, _>>();
        let mut ids = BTreeSet::new();
        let mut usage = Usage {
            bytes: 0,
            records: 0,
            blocked: false,
            names: BTreeSet::new(),
        };
        for name in entries.keys() {
            if id_valid(name) {
                ids.insert(name.clone());
                usage.names.insert(name.clone());
            }
            if let Some(id) = name
                .strip_prefix("parent-cleanup-")
                .and_then(|name| name.strip_suffix(".json"))
                .filter(|id| id_valid(id))
            {
                ids.insert(id.to_string());
                usage.names.insert(name.clone());
            }
        }
        for id in ids {
            let (bytes, mut blocked) = self.parent_group(&id, &entries)?;
            if let Some(binding) = binding.filter(|binding| binding.parent_record == id) {
                self.verify_parent_binding(binding)?;
                blocked = false;
            }
            add(&mut usage.bytes, bytes)?;
            usage.records += 1;
            usage.blocked |= blocked;
        }
        self._lock.revalidate()?;
        Ok(usage)
    }
    fn parent_group(
        &self,
        id: &str,
        entries: &BTreeMap<String, Entry>,
    ) -> Result<(u64, bool), Error> {
        let proof_name = format!("parent-cleanup-{id}.json");
        let proof_size = entries.get(&proof_name).map_or(0, |entry| entry.size);
        let foreign = entries.get(id).filter(|entry| !entry.directory);
        let directory = match entries.get(id) {
            Some(entry) if entry.directory => Some(self.directory.lookup(id)?),
            _ => None,
        };
        let mut native = actual(directory.as_ref(), proof_size)?;
        if let Some(entry) = foreign {
            add(&mut native, entry.size)?;
        }
        let intent = read_group_intent(directory.as_ref(), id);
        if entries.contains_key(&proof_name) {
            let proof = read_group_cleanup(&self.directory, &proof_name, id);
            let Some(proof) = proof else {
                return Ok((
                    intent
                        .as_ref()
                        .map_or(native, |(intent, _)| intent.reserved_bytes.max(native)),
                    true,
                ));
            };
            if foreign.is_some() {
                return Err(Error::invalid("Parent cleanup record directory changed"));
            }
            verify_group_intent(&proof, directory.as_ref(), intent.as_ref())?;
            if native > proof.intent.reserved_bytes {
                return Err(Error::invalid(
                    "Parent cleanup native artifacts exceed the reservation",
                ));
            }
            return Ok((proof.intent.reserved_bytes, true));
        }
        let Some((intent, _)) = intent else {
            return Ok((native, true));
        };
        if native > intent.reserved_bytes {
            return Err(Error::invalid(
                "Parent native artifacts exceed the reservation",
            ));
        }
        let blocked = self
            .load_parent(id)
            .map_or(true, |loaded| !loaded.revision.state.cleanable());
        Ok((intent.reserved_bytes, blocked))
    }
}

fn read_group_intent(directory: Option<&PrivateDir>, id: &str) -> Option<(ParentIntent, Vec<u8>)> {
    directory
        .and_then(|directory| {
            directory
                .file("intent.json", false)
                .and_then(|file| file.read(LIMIT))
                .ok()
        })
        .and_then(|bytes| {
            decode::<ParentIntent>(&bytes)
                .ok()
                .map(|intent| (intent, bytes))
        })
        .filter(|(intent, _)| intent.id == id && intent.validate().is_ok())
}

fn verify_group_intent(
    proof: &ParentCleanup,
    directory: Option<&PrivateDir>,
    intent: Option<&(ParentIntent, Vec<u8>)>,
) -> Result<(), Error> {
    if intent
        .is_some_and(|(_, bytes)| encode(&proof.intent).map_or(true, |encoded| encoded != *bytes))
    {
        return Err(Error::invalid("Parent cleanup and record intents differ"));
    }
    if let Some(directory) = directory {
        verify_cleanup_directory(proof, directory)?;
    }
    Ok(())
}

fn read_group_cleanup(directory: &PrivateDir, name: &str, id: &str) -> Option<ParentCleanup> {
    directory
        .file(name, false)
        .and_then(|file| file.read(LIMIT))
        .ok()
        .and_then(|bytes| decode::<ParentCleanup>(&bytes).ok())
        .filter(|proof| proof.id == id && proof.validate().is_ok())
}
