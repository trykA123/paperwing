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
    if git_binary(bytes) {
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

// Git's convert_is_binary (convert.c gather_stats): CRLF is not converted in such files.
fn git_binary(bytes: &[u8]) -> bool {
    let (mut printable, mut nonprintable) = (0_usize, 0_usize);
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        index += 1;
        match byte {
            b'\r' if bytes.get(index) == Some(&b'\n') => index += 1,
            b'\r' | 0 => return true,
            b'\n' => {}
            b'\x08' | b'\t' | 0x1b | 0x0c => printable += 1,
            0x01..=0x1f | 0x7f => nonprintable += 1,
            _ => printable += 1,
        }
    }
    if bytes.last() == Some(&0x1a) {
        nonprintable -= 1;
    }
    (printable >> 7) < nonprintable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_crlf_conversion_skips_files_git_treats_as_binary() {
        let mut late_nul = vec![b'a'; 8000];
        late_nul.extend_from_slice(b"\0\r\n");
        for bytes in [
            late_nul.as_slice(),
            b"\x01\x01a\r\nb\r\n",
            b"\x7f\r\nb\r\n",
            b"one\rtwo\r\n",
        ] {
            assert!(convert_eol(bytes, true).is_none(), "{bytes:?}");
        }
        for (bytes, converted) in [
            (b"a\tb\x1b\r\nc\r\n".as_slice(), b"a\tb\x1b\nc\n".as_slice()),
            (b"text\r\n\x1a", b"text\n\x1a"),
        ] {
            assert_eq!(convert_eol(bytes, true).unwrap().as_ref(), converted);
        }
    }
}
