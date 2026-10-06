use super::{Error, ReadError, JSON_LIMIT};
use crate::linux_guard::{
    metadata::Security,
    mutation::{AncestorValue, Parent, Snapshot},
    root::RootValue,
    Identity, Root, FILE_LIMIT,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn id_valid(id: &str) -> bool {
    id.len() == 34
        && id.starts_with("r-")
        && id[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Content {
    pub length: u64,
    pub sha256: String,
}
impl Content {
    pub(super) fn of(bytes: &[u8]) -> Self {
        Self {
            length: bytes.len() as u64,
            sha256: hash(bytes),
        }
    }
    pub(super) fn matches(&self, bytes: &[u8]) -> bool {
        self.length == bytes.len() as u64 && self.sha256 == hash(bytes)
    }
    fn valid(&self) -> bool {
        self.length <= FILE_LIMIT as u64 && digest_valid(&self.sha256)
    }
}
pub(super) fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Before {
    pub identity: Identity,
    pub content: Content,
    pub security: Security,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Intent {
    pub id: String,
    pub created_at: u128,
    pub root: RootValue,
    pub path: String,
    pub ancestors: Vec<AncestorValue>,
    pub before: Option<Before>,
    pub after: Content,
    pub after_security: Security,
    pub reverse_of: Option<String>,
}
impl Intent {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if !id_valid(&self.id)
            || self.path.len() > 4096
            || self.ancestors.len() > 63
            || !self.after.valid()
            || self
                .reverse_of
                .as_ref()
                .is_some_and(|id| !id_valid(id) || id == &self.id)
        {
            return Err(Error::invalid("Invalid recovery intent values"));
        }
        crate::paths::relative(&self.path)
            .map_err(|_| Error::invalid("Invalid recovery relative path"))?;
        self.root.validate()?;
        self.after_security.validate()?;
        let parts: Vec<_> = self.path.split('/').collect();
        if parts.len() > 64 || parts.len() != self.ancestors.len() + 1 {
            return Err(Error::invalid("Invalid recovery ancestor count"));
        }
        let mut path = std::path::PathBuf::new();
        for (part, ancestor) in parts.iter().zip(&self.ancestors) {
            path.push(part);
            if ancestor.relative != path {
                return Err(Error::invalid("Invalid recovery ancestor path"));
            }
        }
        if let Some(before) = &self.before {
            if !before.content.valid() {
                return Err(Error::invalid("Invalid recovery before content"));
            }
            before.security.validate()?;
        }
        Ok(())
    }
    pub(super) fn fresh_parent(&self) -> Result<Parent, Error> {
        self.validate()?;
        let root = Root::reopen(&self.root)?;
        let parent = root.parent(&self.path, false)?;
        parent.matches_ancestors(&self.ancestors)?;
        Ok(parent)
    }
    pub(super) fn before_snapshot(&self, bytes: &[u8]) -> Snapshot {
        match &self.before {
            None => Snapshot::Missing,
            Some(before) => Snapshot::Regular {
                identity: before.identity.clone(),
                security: before.security.clone(),
                bytes: bytes.to_vec(),
            },
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    platform: String,
    version: u32,
    payload: T,
    sha256: String,
}
pub(super) fn encode<T: Serialize>(payload: &T) -> Result<Vec<u8>, Error> {
    let checksum = hash(
        &serde_json::to_vec(&("linux", 1u32, payload))
            .map_err(|_| Error::invalid("Recovery serialization failed"))?,
    );
    let bytes = serde_json::to_vec(&Envelope {
        platform: "linux".into(),
        version: 1,
        payload,
        sha256: checksum,
    })
    .map_err(|_| Error::invalid("Recovery serialization failed"))?;
    if bytes.len() > JSON_LIMIT {
        return Err(Error::invalid("Recovery metadata exceeds encoded limit"));
    }
    Ok(bytes)
}
pub(super) fn decode<T: Serialize + DeserializeOwned>(bytes: &[u8]) -> Result<T, ReadError> {
    if bytes.len() > JSON_LIMIT {
        return Err(ReadError::Incomplete);
    }
    let header: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ReadError::Incomplete)?;
    if header.get("platform").and_then(|v| v.as_str()) != Some("linux")
        || header.get("version").and_then(|v| v.as_u64()) != Some(1)
    {
        return Err(ReadError::Foreign);
    }
    let envelope: Envelope<T> = serde_json::from_value(header).map_err(|error| {
        let message = error.to_string();
        if message.contains("unknown field") || message.contains("unknown variant") {
            ReadError::Foreign
        } else {
            ReadError::Incomplete
        }
    })?;
    let encoded = serde_json::to_vec(&("linux", 1u32, &envelope.payload))
        .map_err(|_| ReadError::Incomplete)?;
    if hash(&encoded) != envelope.sha256 {
        return Err(ReadError::Incomplete);
    }
    Ok(envelope.payload)
}
