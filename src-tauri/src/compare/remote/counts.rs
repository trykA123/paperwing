use super::super::{Job, Lines, Problem};

pub(super) async fn count(bytes: [Vec<u8>; 2], job: &Job) -> Result<Option<Lines>, Problem> {
    let job = job.clone();
    tauri::async_runtime::spawn_blocking(move || count_bytes(&bytes[0], &bytes[1], &job))
        .await
        .map_err(|_| Problem::new("githubUnavailable", "GitHub line count task failed"))?
}

fn count_bytes(left: &[u8], right: &[u8], job: &Job) -> Result<Option<Lines>, Problem> {
    job.check()?;
    if super::super::binary(left) || super::super::binary(right) {
        return Ok(None);
    }
    let left: Vec<_> = left.split_inclusive(|byte| *byte == b'\n').collect();
    let right: Vec<_> = right.split_inclusive(|byte| *byte == b'\n').collect();
    let prefix = left
        .iter()
        .zip(&right)
        .take_while(|(left, right)| left == right)
        .count();
    let mut left = &left[prefix..];
    let mut right = &right[prefix..];
    while !left.is_empty() && left.last() == right.last() {
        left = &left[..left.len() - 1];
        right = &right[..right.len() - 1];
    }
    if left.len().saturating_mul(right.len()) > 4_000_000 {
        return Err(Problem::new(
            "limitExceeded",
            "GitHub line count budget exceeded",
        ));
    }
    let mut lengths = vec![0; right.len() + 1];
    for left_line in left {
        job.check()?;
        let mut diagonal = 0;
        for (index, right_line) in right.iter().enumerate() {
            let previous = lengths[index + 1];
            lengths[index + 1] = if left_line == right_line {
                diagonal + 1
            } else {
                lengths[index].max(previous)
            };
            diagonal = previous;
        }
    }
    let common = lengths[right.len()];
    Ok(Some(Lines {
        added: (right.len() - common) as u64,
        removed: (left.len() - common) as u64,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    fn job() -> Job {
        Job {
            rust_counts: None,
            count_root: None,
            readers: Arc::default(),
            context: "github-count-test".into(),
            cancel: Arc::new(AtomicBool::new(false)),
            #[cfg(target_os = "linux")]
            diff: None,
            #[cfg(target_os = "linux")]
            roots: Vec::new(),
            temporary_root: None,
            inventory_started: None,
        }
    }
    #[test]
    fn counts_duplicates_reordering_and_newline_changes() {
        let job = job();
        for (left, right, expected) in [
            (
                "same\na\na\nb\nend\n",
                "same\na\nb\na\nend\n",
                Lines {
                    added: 1,
                    removed: 1,
                },
            ),
            (
                "a\n",
                "a",
                Lines {
                    added: 1,
                    removed: 1,
                },
            ),
            (
                "a\r\n",
                "a\n",
                Lines {
                    added: 1,
                    removed: 1,
                },
            ),
            (
                "",
                "a\nb\n",
                Lines {
                    added: 2,
                    removed: 0,
                },
            ),
            (
                "a\nb\n",
                "",
                Lines {
                    added: 0,
                    removed: 2,
                },
            ),
        ] {
            assert_eq!(
                count_bytes(left.as_bytes(), right.as_bytes(), &job).unwrap(),
                Some(expected)
            );
        }
    }
    #[test]
    fn enforces_binary_budget_and_cancellation_without_sleeping() {
        let job = job();
        assert!(count_bytes(b"a\0", b"b", &job).unwrap().is_none());
        assert_eq!(
            count_bytes(
                "a\n".repeat(2001).as_bytes(),
                "b\n".repeat(2001).as_bytes(),
                &job
            )
            .unwrap_err()
            .kind,
            "limitExceeded"
        );
        job.cancel.store(true, Ordering::Relaxed);
        assert_eq!(count_bytes(b"", b"", &job).unwrap_err().kind, "cancelled");
    }
}
