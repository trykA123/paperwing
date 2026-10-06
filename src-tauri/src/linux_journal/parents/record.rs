use super::{support, Error, ParentPlan};
use crate::linux_guard::{mutation::AncestorValue, storage::FileProof, Identity};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub(in crate::linux_journal) const LIMIT: usize = 65536;
pub(in crate::linux_journal) fn id_valid(id: &str) -> bool {
    id.len() == 34
        && id.starts_with("p-")
        && id[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
pub(in crate::linux_journal) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let bytes = crate::linux_journal::record::encode(value)?;
    if bytes.len() > LIMIT {
        return Err(Error::invalid("Parent record encoding is unsupported"));
    }
    Ok(bytes)
}
pub(in crate::linux_journal) fn decode<T: Serialize + DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, crate::linux_journal::ReadError> {
    if bytes.len() > LIMIT {
        return Err(crate::linux_journal::ReadError::Incomplete);
    }
    crate::linux_journal::record::decode(bytes)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::linux_journal) struct ParentIntent {
    pub id: String,
    pub created_at: u128,
    pub plan: ParentPlan,
    pub directory_mode: u32,
    pub reserved_bytes: u64,
}
impl ParentIntent {
    pub(in crate::linux_journal) fn reservation(count: usize) -> Result<u64, Error> {
        u64::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(10))
            .and_then(|n| n.checked_mul(LIMIT as u64))
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(|| Error::invalid("Parent reservation overflow"))
    }
    pub(in crate::linux_journal) fn revisions(&self) -> u32 {
        2 * self.plan.missing.len() as u32 + 8
    }
    pub(in crate::linux_journal) fn validate(&self) -> Result<(), Error> {
        self.plan.validate()?;
        if !id_valid(&self.id)
            || self.plan.missing.is_empty()
            || self.directory_mode != 448
            || self.reserved_bytes != Self::reservation(self.plan.missing.len())?
        {
            return Err(Error::invalid("Invalid parent intent values"));
        }
        support::prove(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::linux_journal) struct DirectorySecurity {
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::linux_journal) struct ParentCleanup {
    pub id: String,
    pub directory_identity: Identity,
    pub directory_security: DirectorySecurity,
    pub intent: ParentIntent,
    pub intent_sha256: String,
    pub revision: super::state::ParentRevision,
    pub revision_fingerprint: String,
    pub artifacts: Vec<FileProof>,
}
impl ParentCleanup {
    pub(in crate::linux_journal) fn validate(&self) -> Result<(), Error> {
        use crate::linux_journal::record::{digest_valid, hash};
        self.intent.validate()?;
        self.revision.validate(&self.intent)?;
        let security = crate::linux_guard::metadata::Security::new_file();
        if !id_valid(&self.id)
            || self.id != self.intent.id
            || self.directory_security.mode != 448
            || self.directory_security.uid != security.uid
            || self.directory_security.gid != security.gid
            || !self.revision.state.cleanable()
            || self.intent_sha256 != hash(&encode(&self.intent)?)
            || self.revision.intent != self.intent_sha256
            || self.revision_fingerprint != self.revision.fingerprint()?
        {
            return Err(Error::invalid("Invalid parent cleanup proof"));
        }
        let names = std::iter::once("intent.json".to_string())
            .chain((1..=self.revision.sequence).map(crate::linux_journal::state::name))
            .collect::<Vec<_>>();
        if self.artifacts.len() != names.len() {
            return Err(Error::invalid("Parent cleanup artifact count differs"));
        }
        for (artifact, name) in self.artifacts.iter().zip(names) {
            if artifact.name != name
                || artifact.length > LIMIT as u64
                || !digest_valid(&artifact.sha256)
                || artifact.uid != self.directory_security.uid
                || artifact.gid != self.directory_security.gid
                || artifact.mode != 384
            {
                return Err(Error::invalid("Invalid parent cleanup artifact"));
            }
        }
        let revision_hash = hash(&encode(&self.revision)?);
        if self
            .artifacts
            .first()
            .is_none_or(|artifact| artifact.sha256 != self.intent_sha256)
            || self
                .artifacts
                .last()
                .is_none_or(|artifact| artifact.sha256 != revision_hash)
        {
            return Err(Error::invalid("Parent cleanup encoded ownership differs"));
        }
        Ok(())
    }
}
pub(in crate::linux_journal) fn created_valid(
    intent: &ParentIntent,
    created: &[AncestorValue],
) -> Result<(), Error> {
    Ok(intent.plan.validate_created(created)?)
}
