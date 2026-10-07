use super::count_eligibility::EolMode;
use super::{Job, Lines, Problem};
use std::borrow::Cow;
use std::collections::HashSet;

pub(super) fn count(left: &[u8], right: &[u8], job: &Job) -> Result<Option<Lines>, Problem> {
    job.check()?;
    let autocrlf = job.rust_counts == Some(EolMode::AutoCrlf);
    let Some((left, right)) = convert_eol(left, autocrlf).zip(convert_eol(right, autocrlf)) else {
        return Ok(None);
    };
    let (left, right) = (left.as_ref(), right.as_ref());
    let lines = |bytes: &[u8]| bytes.iter().filter(|byte| **byte == b'\n').count();
    if lines(left) > 8192 || lines(right) > 8192 {
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

fn convert_eol(bytes: &[u8], autocrlf: bool) -> Option<Cow<'_, [u8]>> {
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }
    if !autocrlf || !bytes.contains(&b'\r') {
        return Some(Cow::Borrowed(bytes));
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
    {
        return None;
    }
    Some(Cow::Owned(
        bytes
            .iter()
            .copied()
            .filter(|byte| *byte != b'\r')
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_crlf_conversion_uses_gits_binary_probe_boundary() {
        let mut bytes = vec![b'a'; 8000];
        bytes.extend_from_slice(b"\0\r\n");
        assert_eq!(
            convert_eol(&bytes, true).unwrap().as_ref(),
            [vec![b'a'; 8000], b"\0\n".to_vec()].concat()
        );
        bytes[7999] = 0;
        assert!(convert_eol(&bytes, true).is_none());
    }
}
