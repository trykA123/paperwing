use super::{decode, CompareRef, History, Job, Problem, Resolved};
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct HistorySource {
    pub(super) root: PathBuf,
    pub(super) range: String,
}

pub(super) async fn history(
    left: &Resolved,
    right: &Resolved,
    job: &Job,
) -> Result<(History, Option<HistorySource>), Problem> {
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("compare.history", "other");
    let basis = |side: &Resolved| {
        if matches!(side.context.endpoint.reference, CompareRef::WorkingTree) {
            "workingTreeHead"
        } else {
            "commit"
        }
        .to_string()
    };
    let mut result = History {
        available: false,
        reason: None,
        left_count: None,
        right_count: None,
        left_basis: basis(left),
        right_basis: basis(right),
    };
    for side in [left, right] {
        if job
            .output(
                &side.context.root,
                &["rev-parse", "--is-shallow-repository"],
            )
            .await?
            == b"true\n"
        {
            result.reason = Some("shallowHistory".into());
            return Ok((result, None));
        }
    }
    let mut shared = None;
    for side in [left, right] {
        let other = if side.context.root == left.context.root {
            &right.commit
        } else {
            &left.commit
        };
        if job
            .run(
                &side.context.root,
                &["cat-file", "-e", &format!("{other}^{{commit}}")],
                &[0, 128],
            )
            .await?
            .code
            == Some(0)
        {
            shared = Some(side.context.root.clone());
            break;
        }
    }
    let Some(root) = shared else {
        result.reason = Some("historyObjectsUnavailable".into());
        return Ok((result, None));
    };
    if job
        .run(&root, &["merge-base", &left.commit, &right.commit], &[0, 1])
        .await?
        .code
        != Some(0)
    {
        result.reason = Some("unrelatedHistory".into());
        return Ok((result, None));
    }
    let range = format!("{}...{}", left.commit, right.commit);
    let counts = decode(
        &job.output(
            &root,
            &["rev-list", "--left-right", "--count", &range, "--"],
        )
        .await?,
    )?;
    let counts: Vec<_> = counts.split_whitespace().collect();
    if counts.len() != 2 {
        return Err(Problem::new("gitError", "Invalid history counts"));
    }
    result.left_count = counts[0].parse().ok();
    result.right_count = counts[1].parse().ok();
    if result.left_count.is_none() || result.right_count.is_none() {
        return Err(Problem::new("gitError", "Invalid history counts"));
    }
    result.available = true;
    Ok((result, Some(HistorySource { root, range })))
}

