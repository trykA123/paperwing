use super::{
    parents::binding::ParentBinding,
    publication::Reverse,
    record::{self, Intent},
    state::State,
    Error, Journal, ARTIFACT_LIMIT, JSON_LIMIT, RECORD_LIMIT,
};
use crate::linux_guard::{mutation::Parent, Root};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Admission<'a> {
    pub reverse: Option<&'a Reverse<'a>>,
    pub binding: Option<&'a ParentBinding>,
    pub reservation: u64,
}
struct Usage {
    bytes: u64,
    records: usize,
    blocked: bool,
}
struct ParentInventory {
    root: crate::linux_guard::root::RootValue,
    path: String,
    ancestors: Vec<crate::linux_guard::mutation::AncestorValue>,
    owned: BTreeMap<
        String,
        (
            Option<crate::linux_guard::mutation::StageProof>,
            bool,
            String,
        ),
    >,
}
impl ParentInventory {
    fn reopen(&self) -> Result<Parent, Error> {
        let root = Root::reopen(&self.root)?;
        let parent = root.parent(&self.path, false)?;
        parent.matches_ancestors(&self.ancestors)?;
        Ok(parent)
    }
}
impl Journal {
    #[cfg(test)]
    pub(super) fn accounted_bytes(&self) -> Result<u64, Error> {
        Ok(self.usage()?.bytes)
    }
    fn quota(&self) -> u64 {
        #[cfg(test)]
        {
            self.quota
        }
        #[cfg(not(test))]
        {
            super::STORAGE_LIMIT
        }
    }
    fn record_limit(&self) -> usize {
        #[cfg(test)]
        {
            self.records
        }
        #[cfg(not(test))]
        {
            RECORD_LIMIT
        }
    }
    fn usage(&self) -> Result<Usage, Error> {
        self.usage_bound(None)
    }
    fn usage_bound(&self, binding: Option<&ParentBinding>) -> Result<Usage, Error> {
        self._lock.revalidate()?;
        let parent_usage = self.parent_usage_bound(binding)?;
        let mut usage = Usage {
            bytes: parent_usage.bytes,
            records: parent_usage.records,
            blocked: parent_usage.blocked,
        };
        let mut parents = BTreeMap::<std::path::PathBuf, ParentInventory>::new();
        for entry in self.directory.entries(RECORD_LIMIT * 2 + 1)? {
            if parent_usage.names.contains(&entry.name) {
                continue;
            }
            let entry_name = entry.name.clone();
            usage.bytes = usage
                .bytes
                .checked_add(entry.size)
                .ok_or_else(|| Error::invalid("Recovery size overflow"))?;
            if entry.name == "lock" {
                if entry.directory {
                    return Err(Error::invalid("Recovery lock is not a file"));
                }
                continue;
            }
            if !entry.directory {
                usage.blocked = true;
                continue;
            }
            usage.records += 1;
            let directory = self.directory.lookup(&entry.name)?;
            for artifact in directory.entries(ARTIFACT_LIMIT)? {
                if artifact.directory {
                    return Err(Error::invalid(
                        "Nested recovery artifacts cannot be accounted safely",
                    ));
                }
                usage.bytes = usage
                    .bytes
                    .checked_add(artifact.size)
                    .ok_or_else(|| Error::invalid("Recovery size overflow"))?;
            }
            let loaded = self.load(&entry.name);
            let (intent, state) = match loaded {
                Ok(loaded) => {
                    usage.blocked |= !loaded.revision.state.terminal();
                    (Some(loaded.intent), Some(loaded.revision.state))
                }
                Err(_) => {
                    usage.blocked = true;
                    let intent = directory
                        .file("intent.json", false)
                        .and_then(|file| file.read(JSON_LIMIT))
                        .ok()
                        .and_then(|bytes| super::decode::<Intent>(&bytes).ok());
                    (intent, None)
                }
            };
            if let Some(intent) = intent {
                intent.validate()?;
                intent.fresh_parent()?;
                let key = intent
                    .root
                    .path
                    .join(&intent.path)
                    .parent()
                    .ok_or_else(|| Error::invalid("Recovery parent unavailable"))?
                    .to_path_buf();
                let entry = parents.entry(key).or_insert_with(|| ParentInventory {
                    root: intent.root.clone(),
                    path: intent.path.clone(),
                    ancestors: intent.ancestors.clone(),
                    owned: BTreeMap::new(),
                });
                if entry.root != intent.root || entry.ancestors != intent.ancestors {
                    return Err(Error::invalid("Persistent stage parent context differs"));
                }
                if let Some(state) = state {
                    if let Some(candidate) = state.candidate() {
                        if entry
                            .owned
                            .insert(
                                candidate.into(),
                                (state.proof().cloned(), state.terminal(), entry_name.clone()),
                            )
                            .is_some()
                        {
                            usage.blocked = true;
                        }
                    }
                }
            }
        }
        for (_, inventory) in parents {
            let parent = inventory.reopen()?;
            let owned = inventory.owned;
            for stage in parent.stage_artifacts(RECORD_LIMIT)? {
                usage.bytes = usage
                    .bytes
                    .checked_add(stage.size)
                    .ok_or_else(|| Error::invalid("Persistent stage size overflow"))?;
                match owned.get(&stage.name) {
                    Some((Some(proof), terminal, id))
                        if proof.directory == stage.directory
                            && stage
                                .content
                                .as_ref()
                                .is_none_or(|(id, _)| id == &proof.file) =>
                    {
                        usage.blocked |= !*terminal;
                        if stage.content.is_some() {
                            let loaded = self
                                .load(id)
                                .map_err(|_| Error::invalid("Stage owner is not verified"))?;
                            if parent
                                .verify_stage(proof, &loaded.after, &loaded.intent.after_security)
                                .is_err()
                            {
                                usage.blocked = true;
                            }
                        }
                    }
                    _ => usage.blocked = true,
                }
            }
        }
        Ok(usage)
    }
    pub(super) fn reserve(&self, bytes: u64, new_record: bool) -> Result<(), Error> {
        let usage = self.usage()?;
        if usage
            .bytes
            .checked_add(bytes)
            .is_none_or(|total| total > self.quota())
            || usage.records + usize::from(new_record) > self.record_limit()
        {
            return Err(Error::invalid(
                "Linux recovery capacity reached; retained records were not evicted",
            ));
        }
        Ok(())
    }
    pub(super) fn admit_parent(&self, bytes: u64) -> Result<(), Error> {
        let before = self.usage()?;
        for id in self.directory.names(RECORD_LIMIT * 2 + 1)? {
            if record::id_valid(&id) {
                if let Ok(mut loaded) = self.load(&id) {
                    self.reconcile_loaded(&mut loaded)?;
                }
            }
        }
        if before.blocked || self.usage()?.blocked {
            return Err(Error::invalid(
                "Unresolved recovery artifacts block new Linux parents",
            ));
        }
        self.reserve(bytes, true)
    }
    pub(super) fn admit(&self, parent: &Parent, admission: Admission<'_>) -> Result<(), Error> {
        let Admission {
            reverse,
            binding,
            reservation,
        } = admission;
        if reverse.is_some() && binding.is_some() {
            return Err(Error::invalid("Conflicting publication authority"));
        }
        let before = self.usage_bound(binding)?;
        if reverse.is_none() {
            for id in self.directory.names(RECORD_LIMIT * 2 + 1)? {
                if !record::id_valid(&id) {
                    continue;
                }
                if let Ok(mut loaded) = self.load(&id) {
                    self.reconcile_loaded(&mut loaded)?;
                }
            }
            if before.blocked || self.usage_bound(binding)?.blocked {
                return Err(Error::invalid(
                    "Unresolved recovery artifacts block new Linux writes",
                ));
            }
        } else {
            let reverse =
                reverse.ok_or_else(|| Error::invalid("Missing reverse recovery binding"))?;
            let fresh = self
                .load(&reverse.original.intent.id)
                .map_err(|_| Error::invalid("Original reverse recovery record changed"))?;
            if fresh.intent_hash != reverse.original.intent_hash
                || fresh.revision.fingerprint()? != reverse.original.revision.fingerprint()?
                || !record::id_valid(reverse.id)
            {
                return Err(Error::invalid("Reverse recovery binding changed"));
            }
            if !matches!(&fresh.revision.state,State::Undoing{reverse:Some(id),..} if id==reverse.id)
            {
                return Err(Error::invalid(
                    "Reverse recovery was not durably authorized",
                ));
            }
        }
        let mut known = BTreeSet::new();
        for id in self.directory.names(RECORD_LIMIT * 2 + 1)? {
            if let Ok(loaded) = self.load(&id) {
                if let Some(name) = loaded.revision.state.candidate() {
                    known.insert(name.to_string());
                }
            }
        }
        for stage in parent.stage_artifacts(RECORD_LIMIT)? {
            if !known.contains(&stage.name) {
                return Err(Error::invalid(
                    "Unowned persistent stages block Linux writes",
                ));
            }
        }
        self.reserve(reservation, true)
    }
}
