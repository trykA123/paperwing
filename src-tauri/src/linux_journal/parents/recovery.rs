use super::{
    record::{self, ParentCleanup, ParentIntent, LIMIT},
    state::{ParentRevision, ParentState, RetainReason},
    Error, Journal, Loaded,
};
use crate::linux_guard::{mutation::ParentCreation, Root};
use crate::linux_journal::ReadError;
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ParentRecoveryStage {
    Prepared,
    Creating,
    Created,
    Ready,
    Linking,
    Linked,
    Retained,
    Conflict,
    Resolved,
    CleanupPending,
    Incomplete,
    Foreign,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ParentFileReference {
    None,
    MatchingTerminal,
    Missing,
    Unavailable,
    Mismatched,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParentRecoveryRecord {
    pub id: String,
    pub root: Option<PathBuf>,
    pub destination: Option<String>,
    pub stage: ParentRecoveryStage,
    pub created: u32,
    pub created_at: u128,
    pub uncertain: Option<u32>,
    pub file_record: Option<String>,
    pub file_reference: ParentFileReference,
    pub warning: Option<&'static str>,
}
fn stage(state: &ParentState) -> ParentRecoveryStage {
    match state {
        ParentState::Prepared { .. } => ParentRecoveryStage::Prepared,
        ParentState::Creating { .. } => ParentRecoveryStage::Creating,
        ParentState::Created { .. } => ParentRecoveryStage::Created,
        ParentState::Ready { .. } => ParentRecoveryStage::Ready,
        ParentState::Linking { .. } => ParentRecoveryStage::Linking,
        ParentState::Linked { .. } => ParentRecoveryStage::Linked,
        ParentState::Retained { .. } => ParentRecoveryStage::Retained,
        ParentState::Conflict { .. } => ParentRecoveryStage::Conflict,
        ParentState::Resolved { .. } => ParentRecoveryStage::Resolved,
    }
}
fn unreadable(id: &str, error: ReadError) -> ParentRecoveryRecord {
    let stage = if matches!(error, ReadError::Foreign) {
        ParentRecoveryStage::Foreign
    } else {
        ParentRecoveryStage::Incomplete
    };
    ParentRecoveryRecord {
        id: id.into(),
        root: None,
        destination: None,
        stage,
        created: 0,
        created_at: 0,
        uncertain: None,
        file_record: None,
        file_reference: ParentFileReference::Unavailable,
        warning: Some("Unsupported or changed parent artifacts retained"),
    }
}
impl Journal {
    fn parent_reference(
        &self,
        intent: &ParentIntent,
        revision: &ParentRevision,
    ) -> ParentFileReference {
        let Some(id) = revision.state.file_record() else {
            return ParentFileReference::None;
        };
        let names = match self
            .directory
            .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)
        {
            Ok(names) => names,
            Err(_) => return ParentFileReference::Unavailable,
        };
        if !names.iter().any(|name| name == id) {
            return ParentFileReference::Missing;
        }
        let file = match self.load(id) {
            Ok(file) => file,
            Err(_) => return ParentFileReference::Unavailable,
        };
        if (intent.plan.root != file.intent.root
            || intent.plan.destination != file.intent.path
            || intent
                .plan
                .existing
                .iter()
                .chain(revision.state.created())
                .cloned()
                .collect::<Vec<_>>()
                != file.intent.ancestors)
            || matches!(&revision.state, ParentState::Linked { file_intent, .. } if file_intent != &file.intent_hash)
        {
            return ParentFileReference::Mismatched;
        }
        if file.revision.state.terminal() {
            ParentFileReference::MatchingTerminal
        } else {
            ParentFileReference::Unavailable
        }
    }
    fn parent_dto(&self, intent: &ParentIntent, revision: &ParentRevision) -> ParentRecoveryRecord {
        let file_reference = self.parent_reference(intent, revision);
        let warning = match file_reference {
            ParentFileReference::Missing => Some("Referenced file record is absent"),
            ParentFileReference::Unavailable => Some("Referenced file record is unavailable"),
            ParentFileReference::Mismatched => Some("Referenced file record differs"),
            _ => None,
        };
        ParentRecoveryRecord {
            id: intent.id.clone(),
            root: Some(intent.plan.root.path.clone()),
            destination: Some(intent.plan.destination.clone()),
            stage: stage(&revision.state),
            created: revision.state.created().len() as u32,
            created_at: intent.created_at,
            uncertain: revision.state.uncertain(),
            file_record: revision.state.file_record().map(str::to_string),
            file_reference,
            warning,
        }
    }
    fn parent_cleanup_dto(&self, id: &str) -> ParentRecoveryRecord {
        let result = || -> Result<ParentRecoveryRecord, Error> {
            let bytes = self
                .directory
                .file(&format!("parent-cleanup-{id}.json"), false)?
                .read(LIMIT)?;
            let proof: ParentCleanup = record::decode(&bytes)
                .map_err(|_| Error::invalid("Parent cleanup proof is not verified"))?;
            proof.validate()?;
            if proof.id != id {
                return Err(Error::invalid("Parent cleanup proof identity differs"));
            }
            self.verify_parent_cleanup_layout(&proof)?;
            let mut dto = self.parent_dto(&proof.intent, &proof.revision);
            dto.stage = ParentRecoveryStage::CleanupPending;
            dto.warning = Some("Confirmed parent cleanup requires resumption");
            Ok(dto)
        };
        result().unwrap_or_else(|_| unreadable(id, ReadError::Incomplete))
    }
    pub(crate) fn list_parents(&self) -> Result<Vec<ParentRecoveryRecord>, Error> {
        self._lock.revalidate()?;
        let mut rows = BTreeMap::new();
        for name in self
            .directory
            .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)?
        {
            if let Some(id) = name
                .strip_prefix("parent-cleanup-")
                .and_then(|name| name.strip_suffix(".json"))
                .filter(|id| record::id_valid(id))
            {
                rows.insert(id.into(), self.parent_cleanup_dto(id));
                continue;
            }
            if !name.starts_with("p-") && !name.starts_with("parent-cleanup-") {
                continue;
            }
            let row = match self.load_parent(&name) {
                Ok(loaded) => self.parent_dto(&loaded.intent, &loaded.revision),
                Err(error) => unreadable(&name, error),
            };
            rows.entry(row.id.clone()).or_insert(row);
        }
        self._lock.revalidate()?;
        Ok(rows.into_values().collect())
    }
    pub(in crate::linux_journal) fn reconcile_parents(&self) -> Result<(), Error> {
        for id in self
            .directory
            .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)?
        {
            if let Ok(mut loaded) = self.load_parent(&id) {
                self.reconcile_parent(&mut loaded)?;
            }
        }
        Ok(())
    }
    fn reconcile_parent(&self, loaded: &mut Loaded) -> Result<(), Error> {
        if loaded.revision.state.cleanable()
            || matches!(loaded.revision.state, ParentState::Conflict { .. })
        {
            return Ok(());
        }
        let created = loaded.revision.state.created().to_vec();
        let file_record = loaded.revision.state.file_record().map(str::to_string);
        let root = Root::reopen(&loaded.intent.plan.root).and_then(|root| {
            root.verify_parent_prefix(ParentCreation {
                plan: &loaded.intent.plan,
                created: &created,
            })?;
            Ok(root)
        });
        let state = match (&loaded.revision.state, root) {
            (_, Err(_)) => ParentState::Conflict {
                created,
                uncertain: None,
                file_record,
            },
            (ParentState::Creating { next, .. }, Ok(root))
                if root
                    .verify_parent_next(ParentCreation {
                        plan: &loaded.intent.plan,
                        created: &created,
                    })
                    .is_err() =>
            {
                ParentState::Conflict {
                    created,
                    uncertain: Some(*next),
                    file_record,
                }
            }
            (
                ParentState::Linking {
                    file_record: id, ..
                },
                Ok(_),
            ) => self.interrupted_parent_link(loaded, id)?,
            _ => ParentState::Retained {
                created,
                file_record,
                reason: RetainReason::Interrupted,
            },
        };
        self.append_parent(loaded, state)
    }
    fn interrupted_parent_link(&self, loaded: &Loaded, id: &str) -> Result<ParentState, Error> {
        let created = loaded.revision.state.created().to_vec();
        let file_record = Some(id.to_string());
        if !self
            .directory
            .names(crate::linux_journal::RECORD_LIMIT * 2 + 1)?
            .contains(&id.to_string())
        {
            return Ok(ParentState::Retained {
                created,
                file_record,
                reason: RetainReason::Interrupted,
            });
        }
        Ok(match self.linked_file_hash(loaded, id) {
            Ok(file_intent) => ParentState::Linked {
                created,
                file_record: id.to_string(),
                file_intent,
            },
            Err(_) => ParentState::Conflict {
                created,
                uncertain: None,
                file_record,
            },
        })
    }
    pub(crate) fn acknowledge_parent(
        &self,
        id: &str,
        confirmed: bool,
    ) -> Result<ParentRecoveryRecord, Error> {
        if !confirmed {
            return Err(Error::invalid(
                "Parent acknowledgement requires confirmation",
            ));
        }
        let mut loaded = self
            .load_parent(id)
            .map_err(|_| Error::invalid("Parent record is not verified"))?;
        let ParentState::Conflict {
            created,
            uncertain,
            file_record,
        } = &loaded.revision.state
        else {
            return Err(Error::invalid(
                "Only a verified parent conflict can be acknowledged",
            ));
        };
        let state = ParentState::Resolved {
            created: created.clone(),
            uncertain: *uncertain,
            file_record: file_record.clone(),
        };
        self.append_parent(&mut loaded, state)?;
        Ok(self.parent_dto(&loaded.intent, &loaded.revision))
    }
}
