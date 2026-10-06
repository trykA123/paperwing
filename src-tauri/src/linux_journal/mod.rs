mod admission;
mod cleanup;
mod constructors;
#[cfg(test)]
mod parent_tests;
mod parents;
mod provenance;
mod publication;
mod record;
mod recovery;
mod state;
#[cfg(test)]
mod tests;

use crate::linux_guard::{
    self,
    storage::{Lock, PrivateDir},
};
use record::{decode, encode, hash, id_valid, Intent};
use serde::Serialize;
use state::{Revision, State};
use std::path::{Path, PathBuf};

const JSON_LIMIT: usize = 8 * 1024 * 1024;
const STORAGE_LIMIT: u64 = 512 * 1024 * 1024;
const RECORD_LIMIT: usize = 1024;
const STATE_LIMIT: u32 = 4096;
const ARTIFACT_LIMIT: usize = STATE_LIMIT as usize + 4;
#[derive(Clone, Debug)]
pub(crate) struct Error {
    pub message: &'static str,
    pub record: Option<String>,
    pub applied: bool,
    pub code: Option<i32>,
}
impl Error {
    fn invalid(message: &'static str) -> Self {
        Self {
            message,
            record: None,
            applied: false,
            code: None,
        }
    }
    fn at(mut self, id: &str, applied: bool) -> Self {
        self.record = Some(id.into());
        self.applied = applied;
        self
    }
}
impl From<linux_guard::Error> for Error {
    fn from(error: linux_guard::Error) -> Self {
        Self {
            message: error.message,
            record: None,
            applied: false,
            code: error.code,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for Error {}
#[derive(Debug)]
enum ReadError {
    Foreign,
    Incomplete,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryRecord {
    pub id: String,
    pub root: PathBuf,
    pub path: String,
    pub existed: bool,
    pub stage: String,
    pub created_at: u128,
    pub warning: Option<&'static str>,
}
#[derive(Debug)]
struct Loaded {
    directory: PrivateDir,
    intent: Intent,
    intent_hash: String,
    revision: Revision,
    before: Vec<u8>,
    after: Vec<u8>,
}
impl Loaded {
    fn dto(&self) -> RecoveryRecord {
        RecoveryRecord {
            id: self.intent.id.clone(),
            root: self.intent.root.path.clone(),
            path: self.intent.path.clone(),
            existed: self.intent.before.is_some(),
            stage: self.revision.state.name().into(),
            created_at: self.intent.created_at,
            warning: None,
        }
    }
}
pub(crate) struct Journal {
    directory: PrivateDir,
    _lock: Lock,
    guarded: Option<Vec<linux_guard::root::RootValue>>,
    #[cfg(test)]
    fault: Option<&'static str>,
    #[cfg(test)]
    quota: u64,
    #[cfg(test)]
    records: usize,
    #[cfg(test)]
    writer: bool,
}
impl Journal {
    pub(crate) fn open(app_data: &Path) -> Result<Self, Error> {
        linux_guard::storage::local_ext4_parent(app_data)?;
        let directory = PrivateDir::open(app_data, "linux-recovery-v1")?;
        directory.local_ext4()?;
        let lock = directory.lock()?;
        directory.sync()?;
        Ok(Self {
            directory,
            _lock: lock,
            guarded: None,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            quota: STORAGE_LIMIT,
            #[cfg(test)]
            records: RECORD_LIMIT,
            #[cfg(test)]
            writer: true,
        })
    }
    fn record_dir(&self, id: &str) -> Result<PrivateDir, Error> {
        if !id_valid(id) {
            return Err(Error::invalid("Invalid recovery record identity"));
        }
        Ok(self.directory.lookup(id)?)
    }
    fn load(&self, id: &str) -> Result<Loaded, ReadError> {
        let directory = self.record_dir(id).map_err(|_| ReadError::Incomplete)?;
        let names = directory
            .names(ARTIFACT_LIMIT)
            .map_err(|_| ReadError::Incomplete)?;
        let bytes = directory
            .file("intent.json", false)
            .and_then(|f| f.read(JSON_LIMIT))
            .map_err(|_| ReadError::Incomplete)?;
        let intent: Intent = decode(&bytes)?;
        intent.validate().map_err(|_| ReadError::Incomplete)?;
        if intent.id != id {
            return Err(ReadError::Incomplete);
        }
        let intent_hash = hash(&bytes);
        let before = directory
            .file("before", false)
            .and_then(|f| f.read(linux_guard::FILE_LIMIT))
            .map_err(|_| ReadError::Incomplete)?;
        let after = directory
            .file("after", false)
            .and_then(|f| f.read(linux_guard::FILE_LIMIT))
            .map_err(|_| ReadError::Incomplete)?;
        if !intent.after.matches(&after)
            || intent
                .before
                .as_ref()
                .map_or(!before.is_empty(), |v| !v.content.matches(&before))
        {
            return Err(ReadError::Incomplete);
        }
        let mut previous = None;
        let mut latest: Option<Revision> = None;
        let mut count = 0u32;
        for name in names {
            if name == "intent.json" || name == "before" || name == "after" {
                continue;
            }
            if name == "cleanup.json" {
                return Err(ReadError::Incomplete);
            }
            count += 1;
            if name != state::name(count) || count > STATE_LIMIT {
                return Err(ReadError::Incomplete);
            }
            let bytes = directory
                .file(&name, false)
                .and_then(|f| f.read(JSON_LIMIT))
                .map_err(|_| ReadError::Incomplete)?;
            let revision: Revision = decode(&bytes)?;
            revision.validate().map_err(|_| ReadError::Incomplete)?;
            if revision.sequence != count
                || revision.previous != previous
                || revision.intent != intent_hash
            {
                return Err(ReadError::Incomplete);
            }
            if let State::Undoing { reverse, .. } | State::Undone { reverse, .. } = &revision.state
            {
                if reverse.is_some() != intent.before.is_some() || reverse.as_deref() == Some(id) {
                    return Err(ReadError::Incomplete);
                }
            }
            if let Some(previous_revision) = &latest {
                state::transition(&previous_revision.state, &revision.state)
                    .map_err(|_| ReadError::Incomplete)?;
            } else if !matches!(revision.state, State::Prepared { .. }) {
                return Err(ReadError::Incomplete);
            }
            previous = Some(revision.fingerprint().map_err(|_| ReadError::Incomplete)?);
            latest = Some(revision);
        }
        let revision = latest.ok_or(ReadError::Incomplete)?;
        Ok(Loaded {
            directory,
            intent,
            intent_hash,
            revision,
            before,
            after,
        })
    }
    pub(crate) fn list(&self) -> Result<Vec<RecoveryRecord>, Error> {
        self._lock.revalidate()?;
        let mut result = std::collections::BTreeMap::new();
        for id in self.directory.names(RECORD_LIMIT * 2 + 1)? {
            if id == "lock" {
                continue;
            }
            if id.starts_with("p-") || id.starts_with("parent-cleanup-") {
                continue;
            }
            if let Some(record) = self.cleanup_record(&id) {
                result.insert(record.id.clone(), record);
                continue;
            }
            let record = if id_valid(&id) {
                match self.load(&id) {
                    Ok(loaded) => loaded.dto(),
                    Err(error) => unreadable(&id, error),
                }
            } else {
                unreadable(&id, ReadError::Foreign)
            };
            result.entry(record.id.clone()).or_insert(record);
        }
        Ok(result.into_values().collect())
    }
    pub(crate) fn export(&self, id: &str, before: bool) -> Result<Vec<u8>, Error> {
        self._lock.revalidate()?;
        let directory = self.record_dir(id)?;
        let intent: Intent = decode(&directory.file("intent.json", false)?.read(JSON_LIMIT)?)
            .map_err(|_| Error::invalid("Recovery intent is not verified"))?;
        intent.validate()?;
        if intent.id != id {
            return Err(Error::invalid("Recovery intent identity differs"));
        }
        let bytes = directory
            .file(if before { "before" } else { "after" }, false)?
            .read(linux_guard::FILE_LIMIT)?;
        let valid = if before {
            intent
                .before
                .as_ref()
                .map_or(bytes.is_empty(), |value| value.content.matches(&bytes))
        } else {
            intent.after.matches(&bytes)
        };
        if !valid {
            return Err(Error::invalid("Recovery backup is not verified"));
        }
        Ok(bytes)
    }
    fn append(&self, loaded: &mut Loaded, state: State) -> Result<(), Error> {
        state.validate()?;
        state::transition(&loaded.revision.state, &state)?;

        let revision = Revision {
            sequence: loaded
                .revision
                .sequence
                .checked_add(1)
                .filter(|n| *n <= STATE_LIMIT)
                .ok_or_else(|| Error::invalid("Recovery state limit reached"))?,
            previous: Some(loaded.revision.fingerprint()?),
            intent: loaded.intent_hash.clone(),
            state,
        };
        let bytes = encode(&revision)?;
        self.reserve(bytes.len() as u64 + 4096, false)?;
        loaded
            .directory
            .write_new(&state::name(revision.sequence), &bytes)?;
        loaded.revision = revision;
        Ok(())
    }
}
fn unreadable(id: &str, error: ReadError) -> RecoveryRecord {
    let (stage, warning) = match error {
        ReadError::Foreign => ("foreign", "Unsupported recovery format retained"),
        ReadError::Incomplete => (
            "incomplete",
            "Incomplete or changed recovery artifacts retained",
        ),
    };
    RecoveryRecord {
        id: id.into(),
        root: PathBuf::new(),
        path: String::new(),
        existed: false,
        stage: stage.into(),
        created_at: 0,
        warning: Some(warning),
    }
}
