use super::{
    decode, encode, hash,
    record::{id_valid, Content, Intent},
    state::Revision,
    Error, Journal, Loaded, ARTIFACT_LIMIT, JSON_LIMIT, STATE_LIMIT,
};
use crate::linux_guard::Identity;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    name: String,
    identity: Identity,
    content: Content,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    directory: Identity,
    intent: Intent,
    intent_hash: String,
    revision: Revision,
    artifacts: Vec<Artifact>,
}
#[derive(Debug)]
pub(crate) struct CleanupResult {
    pub removed: u32,
    pub complete: bool,
    pub warning: Option<&'static str>,
}
fn name(id: &str) -> String {
    format!("cleanup-{id}.json")
}
impl Manifest {
    fn validate(&self) -> Result<(), Error> {
        self.intent.validate()?;
        self.revision.validate()?;
        if !id_valid(&self.id)
            || self.intent.id != self.id
            || self.artifacts.len() > ARTIFACT_LIMIT
            || !self.revision.state.cleanable()
            || hash(&encode(&self.intent)?) != self.intent_hash
            || self.revision.intent != self.intent_hash
        {
            return Err(Error::invalid("Invalid confirmed cleanup manifest"));
        }
        let expected: Vec<_> = ["after".into(), "before".into(), "intent.json".into()]
            .into_iter()
            .chain((1..=self.revision.sequence).map(super::state::name))
            .collect();
        let actual: Vec<_> = self
            .artifacts
            .iter()
            .map(|artifact| artifact.name.clone())
            .collect();
        if actual != expected || self.revision.sequence > STATE_LIMIT {
            return Err(Error::invalid("Cleanup ownership inventory differs"));
        }
        for artifact in &self.artifacts {
            let limit = if artifact.name == "before" || artifact.name == "after" {
                crate::linux_guard::FILE_LIMIT
            } else {
                JSON_LIMIT
            };
            if artifact.content.length > limit as u64
                || !super::record::digest_valid(&artifact.content.sha256)
            {
                return Err(Error::invalid("Invalid cleanup artifact bounds"));
            }
        }
        if self.artifacts[2].content.sha256 != self.intent_hash {
            return Err(Error::invalid("Cleanup intent ownership differs"));
        }
        Ok(())
    }
}
impl Journal {
    pub(super) fn cleanup_record(&self, name: &str) -> Option<super::RecoveryRecord> {
        let id = name.strip_prefix("cleanup-")?.strip_suffix(".json")?;
        if !id_valid(id) {
            return None;
        }
        let manifest = self
            .directory
            .file(name, false)
            .and_then(|file| file.read(JSON_LIMIT))
            .ok()
            .and_then(|bytes| decode::<Manifest>(&bytes).ok());
        let mut record = super::unreadable(id, super::ReadError::Incomplete);
        record.warning = Some("Confirmed recovery cleanup requires resumption");
        if let Some(manifest) =
            manifest.filter(|manifest| manifest.id == id && manifest.validate().is_ok())
        {
            record.root = manifest.intent.root.path;
            record.path = manifest.intent.path;
            record.existed = manifest.intent.before.is_some();
            record.created_at = manifest.intent.created_at;
        }
        Some(record)
    }

    fn cleanup_manifest(&self, loaded: &Loaded) -> Result<Manifest, Error> {
        if !loaded.revision.state.cleanable() {
            return Err(Error::invalid("Recovery state is not eligible for cleanup"));
        }
        let entries = loaded.directory.entries(ARTIFACT_LIMIT)?;
        let mut artifacts = Vec::new();
        let expected: Vec<_> = ["after".into(), "before".into(), "intent.json".into()]
            .into_iter()
            .chain((1..=loaded.revision.sequence).map(super::state::name))
            .collect();
        if entries.len() != expected.len()
            || entries
                .iter()
                .zip(&expected)
                .any(|(entry, name)| entry.directory || &entry.name != name)
        {
            return Err(Error::invalid("Unknown cleanup artifacts retained"));
        }
        for entry in entries {
            let limit = if entry.name == "before" || entry.name == "after" {
                crate::linux_guard::FILE_LIMIT
            } else {
                JSON_LIMIT
            };
            let file = loaded.directory.file(&entry.name, false)?;
            let bytes = file.read(limit)?;
            artifacts.push(Artifact {
                name: entry.name,
                identity: file.identity()?,
                content: Content::of(&bytes),
            });
        }
        let manifest = Manifest {
            id: loaded.intent.id.clone(),
            directory: loaded.directory.identity()?,
            intent: loaded.intent.clone(),
            intent_hash: loaded.intent_hash.clone(),
            revision: loaded.revision.clone(),
            artifacts,
        };
        manifest.validate()?;
        self.verify_cleanup_stage(&manifest)?;
        Ok(manifest)
    }
    fn verify_cleanup_stage(&self, manifest: &Manifest) -> Result<(), Error> {
        let parent = manifest.intent.fresh_parent()?;
        for stage in parent.stage_artifacts(super::RECORD_LIMIT)? {
            if manifest.revision.state.candidate() != Some(stage.name.as_str()) {
                continue;
            }
            let proof = manifest
                .revision
                .state
                .proof()
                .ok_or_else(|| Error::invalid("Unproved persistent stage retained"))?;
            if proof.directory != stage.directory
                || stage
                    .content
                    .as_ref()
                    .is_some_and(|(identity, _)| identity != &proof.file)
            {
                return Err(Error::invalid("Cleanup stage identity changed"));
            }
            if stage.content.is_some() {
                let after = self
                    .record_dir(&manifest.id)?
                    .file("after", false)?
                    .read(crate::linux_guard::FILE_LIMIT)?;
                if !manifest.intent.after.matches(&after) {
                    return Err(Error::invalid("Cleanup stage backup changed"));
                }
                parent.verify_stage(proof, &after, &manifest.intent.after_security)?;
            }
        }
        Ok(())
    }
    fn resume_cleanup(
        &self,
        manifest: &Manifest,
        manifest_bytes: &[u8],
        manifest_identity: &Identity,
        removed: &mut u32,
    ) -> Result<(), Error> {
        let directory = match self.record_dir(&manifest.id) {
            Ok(directory) => Some(directory),
            Err(error)
                if error.code == Some(libc::ENOENT)
                    || !self
                        .directory
                        .names(super::RECORD_LIMIT * 2 + 1)?
                        .contains(&manifest.id) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        if let Some(directory) = directory {
            if directory.identity()? != manifest.directory {
                return Err(Error::invalid("Cleanup record directory changed"));
            }
            let entries = directory.entries(ARTIFACT_LIMIT)?;
            for entry in &entries {
                let owned = manifest
                    .artifacts
                    .iter()
                    .find(|artifact| artifact.name == entry.name)
                    .ok_or_else(|| Error::invalid("New cleanup artifact retained"))?;
                if entry.directory || owned.identity != entry.identity {
                    return Err(Error::invalid("Changed cleanup artifact retained"));
                }
                let bytes = directory
                    .file(&entry.name, false)?
                    .read(owned.content.length as usize)?;
                if !owned.content.matches(&bytes) {
                    return Err(Error::invalid("Changed cleanup artifact bytes retained"));
                }
            }
            self.verify_cleanup_stage(manifest)?;
            if let Some(proof) = manifest.revision.state.proof() {
                let parent = manifest.intent.fresh_parent()?;
                let stages = parent.stage_artifacts(super::RECORD_LIMIT)?;
                if let Some(stage) = stages.iter().find(|stage| stage.name == proof.name) {
                    let after = if stage.content.is_some() {
                        directory
                            .file("after", false)?
                            .read(crate::linux_guard::FILE_LIMIT)?
                    } else {
                        Vec::new()
                    };
                    if stage.content.is_some() && !manifest.intent.after.matches(&after) {
                        return Err(Error::invalid("Cleanup stage backup changed"));
                    }
                    parent.cleanup_stage(proof, &after, &manifest.intent.after_security)?;
                }
            }
            self.checkpoint("cleanupStage", &manifest.id)?;
            for artifact in &manifest.artifacts {
                if !directory.names(ARTIFACT_LIMIT)?.contains(&artifact.name) {
                    continue;
                }
                let bytes = directory
                    .file(&artifact.name, false)?
                    .read(artifact.content.length as usize)?;
                if !artifact.content.matches(&bytes) {
                    return Err(Error::invalid("Cleanup artifact changed"));
                }
                directory.unlink_owned(&artifact.name, &artifact.identity, &bytes)?;
                *removed += 1;
                self.checkpoint("cleanupArtifact", &manifest.id)?;
            }
            self.checkpoint("cleanupEmpty", &manifest.id)?;
            self.directory
                .remove_empty(&manifest.id, &manifest.directory)?;
            self.checkpoint("cleanupDirectory", &manifest.id)?;
        }
        self.directory
            .unlink_owned(&name(&manifest.id), manifest_identity, manifest_bytes)?;
        self.checkpoint("cleanupManifest", &manifest.id)?;
        Ok(())
    }
    pub(crate) fn cleanup(&self, id: &str, confirmed: bool) -> Result<CleanupResult, Error> {
        if !confirmed {
            return Err(Error::invalid("Recovery cleanup requires confirmation"));
        }
        if !id_valid(id) {
            return Err(Error::invalid("Invalid recovery record identity"));
        }
        self._lock.revalidate()?;
        let manifest_name = name(id);
        let names = self.directory.names(super::RECORD_LIMIT * 2 + 1)?;
        if !names.iter().any(|name| name == id) && !names.contains(&manifest_name) {
            return Ok(CleanupResult {
                removed: 0,
                complete: true,
                warning: None,
            });
        }
        if !names.contains(&manifest_name) {
            let loaded = self
                .load(id)
                .map_err(|_| Error::invalid("Recovery record is not verified"))?;
            let manifest = self.cleanup_manifest(&loaded)?;
            let bytes = encode(&manifest)?;
            self.reserve(bytes.len() as u64 + 4096, false)?;
            self.directory.write_new(&manifest_name, &bytes)?;
            self.checkpoint("cleanupPrepared", id)?;
        }
        let file = self.directory.file(&manifest_name, false)?;
        let bytes = file.read(JSON_LIMIT)?;
        let identity = file.identity()?;
        let manifest: Manifest =
            decode(&bytes).map_err(|_| Error::invalid("Cleanup manifest is not verified"))?;
        manifest.validate()?;
        if manifest.id != id {
            return Err(Error::invalid("Cleanup manifest identity differs"));
        }
        let mut removed = 0;
        let result = self.resume_cleanup(&manifest, &bytes, &identity, &mut removed);
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
