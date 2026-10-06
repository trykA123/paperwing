use super::{
    record::{created_valid, ParentIntent},
    Error,
};
use crate::linux_guard::mutation::AncestorValue;
use crate::linux_journal::record::{digest_valid, hash, id_valid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::linux_journal) enum RetainReason {
    Cancelled,
    Interrupted,
    FileAdmission,
    FilePublication,
    OperationStopped,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "stage",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(in crate::linux_journal) enum ParentState {
    Prepared {
        created: Vec<AncestorValue>,
    },
    Creating {
        created: Vec<AncestorValue>,
        next: u32,
    },
    Created {
        created: Vec<AncestorValue>,
    },
    Ready {
        created: Vec<AncestorValue>,
    },
    Linking {
        created: Vec<AncestorValue>,
        file_record: String,
    },
    Linked {
        created: Vec<AncestorValue>,
        file_record: String,
        file_intent: String,
    },
    Retained {
        created: Vec<AncestorValue>,
        file_record: Option<String>,
        reason: RetainReason,
    },
    Conflict {
        created: Vec<AncestorValue>,
        uncertain: Option<u32>,
        file_record: Option<String>,
    },
    Resolved {
        created: Vec<AncestorValue>,
        uncertain: Option<u32>,
        file_record: Option<String>,
    },
}
impl ParentState {
    pub(in crate::linux_journal) fn created(&self) -> &[AncestorValue] {
        match self {
            Self::Prepared { created }
            | Self::Creating { created, .. }
            | Self::Created { created }
            | Self::Ready { created }
            | Self::Linking { created, .. }
            | Self::Linked { created, .. }
            | Self::Retained { created, .. }
            | Self::Conflict { created, .. }
            | Self::Resolved { created, .. } => created,
        }
    }
    pub(in crate::linux_journal) fn file_record(&self) -> Option<&str> {
        match self {
            Self::Linking { file_record, .. } | Self::Linked { file_record, .. } => {
                Some(file_record)
            }
            Self::Retained { file_record, .. }
            | Self::Conflict { file_record, .. }
            | Self::Resolved { file_record, .. } => file_record.as_deref(),
            _ => None,
        }
    }
    pub(in crate::linux_journal) fn uncertain(&self) -> Option<u32> {
        match self {
            Self::Conflict { uncertain, .. } | Self::Resolved { uncertain, .. } => *uncertain,
            _ => None,
        }
    }
    pub(in crate::linux_journal) fn cleanable(&self) -> bool {
        matches!(
            self,
            Self::Linked { .. } | Self::Retained { .. } | Self::Resolved { .. }
        )
    }
    pub(in crate::linux_journal) fn validate(&self, intent: &ParentIntent) -> Result<(), Error> {
        let created = self.created();
        created_valid(intent, created)?;
        if self.file_record().is_some_and(|id| !id_valid(id))
            || self.uncertain().is_some_and(|index| {
                index as usize != created.len() || created.len() >= intent.plan.missing.len()
            })
        {
            return Err(Error::invalid("Invalid parent state binding"));
        }
        let valid = match self {
            Self::Prepared { .. } => created.is_empty(),
            Self::Creating { next, .. } => {
                *next as usize == created.len() && created.len() < intent.plan.missing.len()
            }
            Self::Created { .. } => !created.is_empty(),
            Self::Ready { .. } | Self::Linking { .. } => created.len() == intent.plan.missing.len(),
            Self::Linked { file_intent, .. } => {
                created.len() == intent.plan.missing.len() && digest_valid(file_intent)
            }
            _ => true,
        };
        if !valid {
            return Err(Error::invalid("Invalid parent state prefix"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::linux_journal) struct ParentRevision {
    pub sequence: u32,
    pub previous: Option<String>,
    pub intent: String,
    pub state: ParentState,
}
impl ParentRevision {
    pub(in crate::linux_journal) fn validate(&self, intent: &ParentIntent) -> Result<(), Error> {
        if self.sequence == 0
            || self.sequence > intent.revisions()
            || !digest_valid(&self.intent)
            || self
                .previous
                .as_ref()
                .is_some_and(|value| !digest_valid(value))
            || (self.sequence == 1) != self.previous.is_none()
        {
            return Err(Error::invalid("Invalid parent revision chain"));
        }
        self.state.validate(intent)
    }
    pub(in crate::linux_journal) fn fingerprint(&self) -> Result<String, Error> {
        Ok(hash(&serde_json::to_vec(self).map_err(|_| {
            Error::invalid("Parent state serialization failed")
        })?))
    }
}
pub(in crate::linux_journal) fn transition(
    before: &ParentState,
    after: &ParentState,
) -> Result<(), Error> {
    let same = before.created() == after.created();
    let valid = match (before, after) {
        (
            ParentState::Prepared { .. } | ParentState::Created { .. },
            ParentState::Creating { .. },
        ) => same,
        (ParentState::Creating { created, .. }, ParentState::Created { created: next }) => {
            next.len() == created.len() + 1 && next.starts_with(created)
        }
        (ParentState::Created { .. }, ParentState::Ready { .. }) => same,
        (ParentState::Ready { .. }, ParentState::Linking { .. }) => same,
        (
            ParentState::Linking { file_record, .. },
            ParentState::Linked {
                file_record: next, ..
            },
        ) => same && file_record == next,
        (
            ParentState::Conflict {
                created,
                uncertain,
                file_record,
            },
            ParentState::Resolved {
                created: next,
                uncertain: next_uncertain,
                file_record: next_file,
            },
        ) => created == next && uncertain == next_uncertain && file_record == next_file,
        (
            ParentState::Prepared { .. }
            | ParentState::Creating { .. }
            | ParentState::Created { .. }
            | ParentState::Ready { .. }
            | ParentState::Linking { .. },
            ParentState::Retained { .. } | ParentState::Conflict { .. },
        ) => {
            same && (before.file_record().is_none() || before.file_record() == after.file_record())
        }
        _ => false,
    };
    if !valid {
        return Err(Error::invalid("Invalid parent state transition"));
    }
    Ok(())
}
