use super::{
    record::{DirectorySecurity, ParentCleanup, ParentIntent, LIMIT},
    state::{ParentRevision, ParentState, RetainReason},
    Error,
};
use crate::linux_guard::{mutation::AncestorValue, storage::FileProof, Identity};

pub(in crate::linux_journal) fn prove(intent: &ParentIntent) -> Result<(), Error> {
    if maximum_size(intent)? > LIMIT {
        return Err(Error::invalid("Parent record encoding is unsupported"));
    }
    Ok(())
}
fn initial_states(created: &[AncestorValue]) -> Vec<ParentState> {
    vec![
        ParentState::Prepared {
            created: Vec::new(),
        },
        ParentState::Creating {
            created: created[..created.len().saturating_sub(1)].to_vec(),
            next: created.len().saturating_sub(1) as u32,
        },
        ParentState::Created {
            created: created.to_vec(),
        },
        ParentState::Ready {
            created: created.to_vec(),
        },
    ]
}
fn file_states(created: &[AncestorValue], id: &str, digest: &str) -> Vec<ParentState> {
    vec![
        ParentState::Linking {
            created: created.to_vec(),
            file_record: id.into(),
        },
        ParentState::Linked {
            created: created.to_vec(),
            file_record: id.into(),
            file_intent: digest.into(),
        },
    ]
}
fn retained_states(created: &[AncestorValue], id: &str) -> Vec<ParentState> {
    let mut states = Vec::new();
    for reason in [
        RetainReason::Cancelled,
        RetainReason::Interrupted,
        RetainReason::FileAdmission,
        RetainReason::FilePublication,
        RetainReason::OperationStopped,
    ] {
        for file_record in [None, Some(id.into())] {
            states.push(ParentState::Retained {
                created: created.to_vec(),
                file_record,
                reason,
            });
        }
    }
    states
}
fn conflict_states(created: &[AncestorValue], id: &str) -> Vec<ParentState> {
    let mut states = Vec::new();
    for uncertain in [None, Some(created.len().saturating_sub(1) as u32)] {
        for file_record in [None, Some(id.into())] {
            let prefix = if uncertain.is_some() {
                created[..created.len().saturating_sub(1)].to_vec()
            } else {
                created.to_vec()
            };
            states.push(ParentState::Conflict {
                created: prefix.clone(),
                uncertain,
                file_record: file_record.clone(),
            });
            states.push(ParentState::Resolved {
                created: prefix,
                uncertain,
                file_record,
            });
        }
    }
    states
}
fn maximum_artifacts(intent: &ParentIntent) -> Vec<FileProof> {
    std::iter::once("intent.json".to_string())
        .chain((1..=intent.revisions()).map(crate::linux_journal::state::name))
        .map(|name| FileProof {
            name,
            identity: Identity::maximum_width(),
            length: LIMIT as u64,
            sha256: "f".repeat(64),
            uid: u32::MAX,
            gid: u32::MAX,
            mode: u32::MAX,
        })
        .collect()
}
fn maximum_cleanup(
    intent: &ParentIntent,
    revision: ParentRevision,
    artifacts: &[FileProof],
) -> ParentCleanup {
    ParentCleanup {
        id: intent.id.clone(),
        directory_identity: Identity::maximum_width(),
        directory_security: DirectorySecurity {
            uid: u32::MAX,
            gid: u32::MAX,
            mode: u32::MAX,
        },
        intent: intent.clone(),
        intent_sha256: "f".repeat(64),
        revision,
        revision_fingerprint: "f".repeat(64),
        artifacts: artifacts.to_vec(),
    }
}
pub(in crate::linux_journal) fn maximum_size(intent: &ParentIntent) -> Result<usize, Error> {
    let mut maximum = crate::linux_journal::record::encode(intent)?.len();
    let created = intent
        .plan
        .missing
        .iter()
        .map(|path| AncestorValue {
            relative: path.into(),
            identity: Identity::maximum_width(),
        })
        .collect::<Vec<_>>();
    let id = format!("r-{}", "f".repeat(32));
    let digest = "f".repeat(64);
    let states = initial_states(&created)
        .into_iter()
        .chain(file_states(&created, &id, &digest))
        .chain(retained_states(&created, &id))
        .chain(conflict_states(&created, &id));
    let artifacts = maximum_artifacts(intent);
    for state in states {
        let revision = ParentRevision {
            sequence: intent.revisions(),
            previous: Some(digest.clone()),
            intent: digest.clone(),
            state,
        };
        maximum = maximum.max(crate::linux_journal::record::encode(&revision)?.len());
        maximum = maximum.max(
            crate::linux_journal::record::encode(&maximum_cleanup(intent, revision, &artifacts))?
                .len(),
        );
    }
    Ok(maximum)
}
