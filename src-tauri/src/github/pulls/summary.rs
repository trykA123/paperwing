use super::model::{CheckRun, Checks, ChecksState, CommitStatus, PullState, Review, ReviewState};
use std::collections::HashMap;

pub(super) fn review_state(reviews: &[Review], state: PullState) -> ReviewState {
    let mut latest: HashMap<u64, &Review> = HashMap::new();
    for review in reviews {
        if !matches!(review.state.as_str(), "APPROVED" | "CHANGES_REQUESTED") {
            continue;
        }
        let Some(user) = &review.user else {
            continue;
        };
        let previous = latest.entry(user.id).or_insert(review);
        if (&review.submitted_at, review.id) > (&previous.submitted_at, previous.id) {
            *previous = review;
        }
    }
    if latest
        .values()
        .any(|review| review.state == "CHANGES_REQUESTED")
    {
        return ReviewState::ChangesRequested;
    }
    if !latest.is_empty() {
        return ReviewState::Approved;
    }
    if matches!(state, PullState::Open | PullState::Draft) {
        ReviewState::ReviewRequired
    } else {
        ReviewState::None
    }
}

pub(super) fn checks(runs: Vec<CheckRun>, statuses: Vec<CommitStatus>) -> Checks {
    let mut latest: HashMap<String, CommitStatus> = HashMap::new();
    for status in statuses {
        if latest
            .get(&status.context)
            .is_some_and(|previous| previous.id >= status.id)
        {
            continue;
        }
        latest.insert(status.context.clone(), status);
    }
    let states = runs
        .iter()
        .map(check_state)
        .chain(latest.values().map(|status| match status.state.as_str() {
            "success" => ChecksState::Success,
            "failure" | "error" => ChecksState::Failure,
            _ => ChecksState::Pending,
        }));
    let mut result = Checks {
        state: ChecksState::None,
        success: 0,
        failure: 0,
        pending: 0,
        total: 0,
    };
    for state in states {
        match state {
            ChecksState::Success => result.success += 1,
            ChecksState::Failure => result.failure += 1,
            ChecksState::Pending => result.pending += 1,
            ChecksState::None => continue,
        }
        result.total += 1;
    }
    result.state = if result.failure > 0 {
        ChecksState::Failure
    } else if result.pending > 0 {
        ChecksState::Pending
    } else if result.success > 0 {
        ChecksState::Success
    } else {
        ChecksState::None
    };
    result
}

fn check_state(run: &CheckRun) -> ChecksState {
    if run.status != "completed" {
        return ChecksState::Pending;
    }
    match run.conclusion.as_deref() {
        Some("success" | "neutral" | "skipped") => ChecksState::Success,
        Some(
            "failure" | "timed_out" | "cancelled" | "action_required" | "startup_failure" | "stale",
        ) => ChecksState::Failure,
        _ => ChecksState::Pending,
    }
}
