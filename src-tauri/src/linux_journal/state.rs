use super::{
    record::{digest_valid, hash, id_valid},
    Error, STATE_LIMIT,
};
use crate::linux_guard::{
    mutation::{StageProof, STAGE_PREFIX},
    Identity,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum State {
    Prepared {
        candidate: String,
    },
    Staged {
        proof: StageProof,
    },
    Replacing {
        proof: StageProof,
    },
    Applied {
        proof: StageProof,
        identity: Identity,
    },
    Undoing {
        proof: StageProof,
        identity: Identity,
        provenance: Vec<super::provenance::Edge>,
        reverse: Option<String>,
    },
    Undone {
        proof: StageProof,
        identity: Identity,
        provenance: Vec<super::provenance::Edge>,
        reverse: Option<String>,
    },
    NotApplied {
        proof: Option<StageProof>,
        candidate: String,
    },
    Conflict {
        proof: Option<StageProof>,
    },
    Resolved {
        proof: Option<StageProof>,
    },
}
impl State {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Prepared { .. } => "prepared",
            Self::Staged { .. } => "staged",
            Self::Replacing { .. } => "replacing",
            Self::Applied { .. } => "applied",
            Self::Undoing { .. } => "undoing",
            Self::Undone { .. } => "undone",
            Self::NotApplied { .. } => "notApplied",
            Self::Conflict { .. } => "conflict",
            Self::Resolved { .. } => "resolved",
        }
    }
    pub(super) fn terminal(&self) -> bool {
        matches!(
            self,
            Self::Applied { .. }
                | Self::Undone { .. }
                | Self::NotApplied { .. }
                | Self::Resolved { .. }
        )
    }
    pub(super) fn cleanable(&self) -> bool {
        matches!(
            self,
            Self::Undone { .. } | Self::NotApplied { .. } | Self::Resolved { .. }
        )
    }
    pub(super) fn proof(&self) -> Option<&StageProof> {
        match self {
            Self::Prepared { .. } => None,
            Self::Staged { proof }
            | Self::Replacing { proof }
            | Self::Applied { proof, .. }
            | Self::Undoing { proof, .. }
            | Self::Undone { proof, .. } => Some(proof),
            Self::NotApplied { proof, .. }
            | Self::Conflict { proof }
            | Self::Resolved { proof } => proof.as_ref(),
        }
    }
    pub(super) fn candidate(&self) -> Option<&str> {
        match self {
            Self::Prepared { candidate } | Self::NotApplied { candidate, .. } => Some(candidate),
            _ => self.proof().map(|proof| proof.name.as_str()),
        }
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        if let Some(name) = self.candidate() {
            if name.len() != STAGE_PREFIX.len() + 32
                || !name.starts_with(STAGE_PREFIX)
                || !name[STAGE_PREFIX.len()..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::invalid("Invalid persistent stage name"));
            }
        }
        if let Self::Undoing {
            reverse: Some(id), ..
        }
        | Self::Undone {
            reverse: Some(id), ..
        } = self
        {
            if !id_valid(id) {
                return Err(Error::invalid("Invalid reverse record identity"));
            }
        }
        if let Self::Applied { proof, identity } = self {
            if &proof.file != identity {
                return Err(Error::invalid(
                    "Applied identity differs from staged identity",
                ));
            }
        }
        if let Self::Undoing {
            proof,
            identity,
            provenance,
            ..
        }
        | Self::Undone {
            proof,
            identity,
            provenance,
            ..
        } = self
        {
            super::provenance::validate_path(&proof.file, identity, provenance)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Revision {
    pub sequence: u32,
    pub previous: Option<String>,
    pub intent: String,
    pub state: State,
}
impl Revision {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.sequence == 0
            || self.sequence > STATE_LIMIT
            || !digest_valid(&self.intent)
            || self.previous.as_ref().is_some_and(|v| !digest_valid(v))
        {
            return Err(Error::invalid("Invalid recovery state chain values"));
        }
        self.state.validate()
    }
    pub(super) fn fingerprint(&self) -> Result<String, Error> {
        Ok(hash(&serde_json::to_vec(self).map_err(|_| {
            Error::invalid("State serialization failed")
        })?))
    }
}
pub(super) fn name(sequence: u32) -> String {
    format!("state-{sequence:04}.json")
}

pub(super) fn transition(before: &State, after: &State) -> Result<(), Error> {
    let allowed = match before {
        State::Prepared { .. } => matches!(
            after,
            State::Staged { .. } | State::NotApplied { .. } | State::Conflict { .. }
        ),
        State::Staged { .. } => matches!(
            after,
            State::Replacing { .. }
                | State::Applied { .. }
                | State::NotApplied { .. }
                | State::Conflict { .. }
        ),
        State::Replacing { .. } => matches!(
            after,
            State::Applied { .. } | State::NotApplied { .. } | State::Conflict { .. }
        ),
        State::Applied { .. } => matches!(after, State::Undoing { .. } | State::Conflict { .. }),
        State::Undoing { .. } => matches!(
            after,
            State::Undoing { .. } | State::Undone { .. } | State::Conflict { .. }
        ),
        State::Conflict { .. } => matches!(after, State::Resolved { .. }),
        _ => false,
    };
    if let (
        State::Undoing {
            identity: a,
            provenance: p,
            ..
        },
        State::Undoing {
            identity: b,
            provenance: q,
            ..
        }
        | State::Undone {
            identity: b,
            provenance: q,
            ..
        },
    ) = (before, after)
    {
        if a != b || p != q {
            return Err(Error::invalid("Undo provenance changed during restoration"));
        }
    }
    if !allowed
        || before
            .candidate()
            .zip(after.candidate())
            .is_some_and(|(a, b)| a != b)
        || before
            .proof()
            .is_some_and(|proof| after.proof() != Some(proof))
    {
        return Err(Error::invalid("Invalid recovery state transition"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn stage_names_accept_skein_prefix() {
        let state = State::Prepared {
            candidate: ".skein-stage-0123456789abcdef0123456789abcdef".into(),
        };
        assert!(state.validate().is_ok());
    }

    #[test]
    fn stage_names_reject_malformed_names() {
        for candidate in [
            ".other-stage-0123456789abcdef0123456789abcdef",
            ".skein-stage-0123456789abcdef0123456789abcde",
            ".skein-stage-0123456789abcdef0123456789abcdef0",
            ".skein-stage-0123456789abcdef0123456789abcdeF",
            ".skein-stage-0123456789abcdef0123456789abcdeg",
            ".skein-stage-0123456789abcdef0123456789abcde/",
        ] {
            let state = State::Prepared {
                candidate: candidate.into(),
            };
            assert!(
                state.validate().is_err(),
                "accepted invalid stage: {candidate}"
            );
        }
    }
}
