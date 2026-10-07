use super::{Job, Lines, Problem};
use std::collections::HashSet;

pub(super) fn count(left: &[u8], right: &[u8], job: &Job) -> Result<Option<Lines>, Problem> {
    job.check()?;
    let lines = |bytes: &[u8]| bytes.iter().filter(|byte| **byte == b'\n').count();
    if left.contains(&b'\r') || right.contains(&b'\r') || lines(left) > 8192 || lines(right) > 8192
    {
        return Ok(None);
    }
    let mut left: Vec<_> = left.split_inclusive(|byte| *byte == b'\n').collect();
    let mut right: Vec<_> = right.split_inclusive(|byte| *byte == b'\n').collect();
    let prefix = left
        .iter()
        .zip(&right)
        .take_while(|(left, right)| left == right)
        .count();
    let mut left = left.drain(prefix..).collect::<Vec<_>>();
    let mut right = right.drain(prefix..).collect::<Vec<_>>();
    while left.last().is_some() && left.last() == right.last() {
        left.pop();
        right.pop();
    }
    let unique: HashSet<_> = left.iter().copied().collect();
    job.check()?;
    if right.iter().any(|line| unique.contains(line)) {
        return Ok(None);
    }
    Ok(Some(Lines {
        added: right.len() as u64,
        removed: left.len() as u64,
    }))
}
