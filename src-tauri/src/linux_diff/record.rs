use super::Error;
use crate::linux_guard::{storage::FileProof, Identity, FILE_LIMIT};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) const JSON_LIMIT: usize = 64 * 1024;
pub(super) const OVERHEAD: u64 = 143360;
pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
pub(super) fn id_valid(value: &str) -> bool {
    value.len() == 34
        && value.starts_with("d-")
        && value[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlannedFile {
    pub name: String,
    pub length: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Claim {
    pub namespace_identity: Identity,
    pub directory_name: String,
    pub reserved_bytes: u64,
    pub files: Vec<PlannedFile>,
}
impl Claim {
    pub(super) fn new(
        namespace_identity: Identity,
        directory_name: String,
        bytes: &[Vec<u8>; 2],
    ) -> Result<Self, Error> {
        let files = ["left", "right"]
            .into_iter()
            .zip(bytes)
            .map(|(name, bytes)| PlannedFile {
                name: name.into(),
                length: bytes.len() as u64,
                sha256: hash(bytes),
            })
            .collect::<Vec<_>>();
        let reserved_bytes = files
            .iter()
            .try_fold(OVERHEAD, |sum, file| sum.checked_add(file.length))
            .ok_or_else(|| Error::unavailable("Private diff reservation overflow"))?;
        let claim = Self {
            namespace_identity,
            directory_name,
            reserved_bytes,
            files,
        };
        claim.validate()?;
        Ok(claim)
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        if !id_valid(&self.directory_name) || self.files.len() != 2 {
            return Err(Error::unavailable("Invalid private diff claim"));
        }
        let mut expected = OVERHEAD;
        for (file, name) in self.files.iter().zip(["left", "right"]) {
            if file.name != name || file.length > FILE_LIMIT as u64 || !digest_valid(&file.sha256) {
                return Err(Error::unavailable("Invalid private diff file claim"));
            }
            expected = expected
                .checked_add(file.length)
                .ok_or_else(|| Error::unavailable("Private diff reservation overflow"))?;
        }
        if self.reserved_bytes != expected {
            return Err(Error::unavailable("Invalid private diff reservation"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Manifest {
    pub claim_sha256: String,
    pub directory_identity: Identity,
    pub files: Vec<FileProof>,
}
impl Manifest {
    pub(super) fn validate(&self, claim: &Claim, claim_bytes: &[u8]) -> Result<(), Error> {
        if self.claim_sha256 != hash(claim_bytes) || self.files.len() != 2 {
            return Err(Error::unavailable(
                "Invalid private diff allocation manifest",
            ));
        }
        // SAFETY: These argument-free libc calls only read process credentials.
        let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
        for (proof, planned) in self.files.iter().zip(&claim.files) {
            if proof.name != planned.name
                || proof.length != planned.length
                || proof.sha256 != planned.sha256
                || proof.mode != 0o600
                || proof.uid != uid
                || proof.gid != gid
            {
                return Err(Error::unavailable(
                    "Private diff manifest disagrees with claim",
                ));
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    platform: String,
    version: u32,
    sha256: String,
    payload: T,
}
pub(super) fn encode<T: Serialize>(payload: &T) -> Result<Vec<u8>, Error> {
    let canonical = serde_json::to_vec(payload)
        .map_err(|_| Error::unavailable("Private diff encoding failed"))?;
    let bytes = serde_json::to_vec(&Envelope {
        platform: "linux".into(),
        version: 1,
        sha256: hash(&canonical),
        payload,
    })
    .map_err(|_| Error::unavailable("Private diff encoding failed"))?;
    if bytes.len() > JSON_LIMIT {
        return Err(Error::unavailable(
            "Private diff metadata exceeds the bound",
        ));
    }
    Ok(bytes)
}
pub(super) fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > JSON_LIMIT {
        return Err(Error::unavailable(
            "Private diff metadata exceeds the bound",
        ));
    }
    let envelope: Envelope<T> = serde_json::from_slice(bytes)
        .map_err(|_| Error::unavailable("Invalid private diff metadata"))?;
    if envelope.platform != "linux"
        || envelope.version != 1
        || !digest_valid(&envelope.sha256)
        || hash(
            &serde_json::to_vec(&envelope.payload)
                .map_err(|_| Error::unavailable("Private diff encoding failed"))?,
        ) != envelope.sha256
    {
        return Err(Error::unavailable("Private diff metadata checksum failed"));
    }
    Ok(envelope.payload)
}
