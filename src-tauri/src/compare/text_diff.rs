use super::{decode, CompareRef, Job, Lines, Options, Problem, Rename, Resolved};
use crate::git;
use std::collections::HashMap;
#[cfg(not(target_os = "linux"))]
use std::path::PathBuf;
#[cfg(not(target_os = "linux"))]
use super::NEXT;
#[cfg(not(target_os = "linux"))]
use std::sync::atomic::Ordering;

pub(super) fn normalized(bytes: &[u8], options: &Options) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    if bytes.contains(&0) {
        return bytes.to_vec();
    }
    let text = if options.normalize_eol {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    };
    if options.ignore_whitespace {
        text.chars()
            .filter(|character| {
                *character == '\r' || *character == '\n' || !character.is_whitespace()
            })
            .collect::<String>()
            .into_bytes()
    } else {
        text.into_bytes()
    }
}

pub(super) fn binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

#[cfg(not(target_os = "linux"))]
struct Temporary(PathBuf);
#[cfg(not(target_os = "linux"))]
impl Temporary {
    fn new(_job: &Job) -> Result<Self, Problem> {
        #[cfg(not(test))]
        let parent = std::env::temp_dir();
        #[cfg(test)]
        let parent = _job
            .temporary_root
            .clone()
            .unwrap_or_else(std::env::temp_dir);
        git::valid_path(
            parent
                .to_str()
                .ok_or_else(|| Problem::new("unsafePath", "Unsupported temporary root"))?,
            true,
        )
        .map_err(|error| Problem::new("unsafePath", &error))?;
        #[cfg(not(target_os = "linux"))]
        {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = parent.join(format!(
            "skein-diff-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|_| {
            Problem::new(
                "unavailable",
                "Could not create private diff materialization",
            )
        })?;
        Ok(Self(path))
        }
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<PathBuf, Problem> {
        #[cfg(not(target_os = "linux"))]
        {
        use std::io::Write;
        let path = self.0.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| Problem::new("unavailable", "Could not materialize diff content"))?;
        file.write_all(bytes)
            .map_err(|_| Problem::new("unavailable", "Could not materialize diff content"))?;
        Ok(path)
        }
    }
}
#[cfg(not(target_os = "linux"))]
impl Drop for Temporary {
    fn drop(&mut self) {
        #[cfg(not(target_os = "linux"))]
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn line_counts(left: &[u8], right: &[u8], job: &Job) -> Result<Option<Lines>, Problem> {
    if binary(left) || binary(right) {
        return Ok(None);
    }
    #[cfg(target_os = "linux")]
    let temporary = job.diff.as_ref().ok_or_else(|| Problem::new("unavailable", "Private diff storage is not configured"))?
        .materialize(job.roots.clone(), job.cancel.clone(), [left, right]).await
        .map_err(storage_problem)?;
    #[cfg(target_os = "linux")]
    let (root, left, right) = (temporary.path().to_path_buf(), temporary.path().join("left"), temporary.path().join("right"));
    #[cfg(not(target_os = "linux"))]
    let mut temporary = Temporary::new(job)?;
    #[cfg(not(target_os = "linux"))]
    let (root, left, right) = (temporary.0.clone(), temporary.write("left", left)?, temporary.write("right", right)?);
    let result = job.run(&root, &[
        "-c", "core.attributesFile=", "diff", "--no-index", "--no-ext-diff", "--no-textconv",
        "--no-renames", "--numstat", "-z", "--",
        left.to_str().ok_or_else(|| Problem::new("unsafePath", "Unsupported diff path"))?,
        right.to_str().ok_or_else(|| Problem::new("unsafePath", "Unsupported diff path"))?,
    ], &[0, 1]).await;
    #[cfg(target_os = "linux")]
    let cleanup = temporary.finish().await.map_err(storage_problem);
    let result = result?;
    #[cfg(target_os = "linux")]
    cleanup?;
    numstat(result)
}
#[cfg(target_os = "linux")]
fn storage_problem(error: crate::linux_diff::Error) -> Problem {
    Problem::new(if error.cancelled { "cancelled" } else { "unavailable" }, error.message)
}
fn numstat(result: git::Captured) -> Result<Option<Lines>, Problem> {
    if !matches!(result.code, Some(0 | 1)) {
        return Err(Problem::new("gitError", "No-index diff failed"));
    }
    if result.stdout.is_empty() {
        return Ok(Some(Lines {
            added: 0,
            removed: 0,
        }));
    }
    let fields: Vec<_> = result.stdout.splitn(3, |byte| *byte == b'\t').collect();
    if fields.len() != 3 {
        return Err(Problem::new("gitError", "Invalid numstat output"));
    }
    if fields[0] == b"-" || fields[1] == b"-" {
        return Ok(None);
    }
    Ok(Some(Lines {
        added: decode(fields[0])?
            .parse()
            .map_err(|_| Problem::new("gitError", "Invalid added count"))?,
        removed: decode(fields[1])?
            .parse()
            .map_err(|_| Problem::new("gitError", "Invalid removed count"))?,
    }))
}

#[derive(Default)]
pub(super) struct DiffMetadata {
    pub(super) lines: HashMap<String, Option<Lines>>,
    pub(super) renames: HashMap<String, Rename>,
    pub(super) reason: Option<String>,
}

pub(super) fn count_result(
    result: Result<Option<Lines>, Problem>,
    reason: &mut Option<String>,
) -> Result<Option<Lines>, Problem> {
    match result {
        Ok(lines) => Ok(lines),
        Err(problem) if problem.kind == "cancelled" => Err(problem),
        Err(problem) => {
            let message = format!("Line counts unavailable: {}", problem.message);
            *reason = Some(
                reason
                    .as_ref()
                    .map(|reason| format!("{reason}; {message}"))
                    .unwrap_or(message),
            );
            Ok(None)
        }
    }
}

pub(super) async fn diff_metadata(
    left: &Resolved,
    right: &Resolved,
    job: &Job,
) -> Result<DiffMetadata, Problem> {
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("compare.metadata", "other");
    let mut metadata = DiffMetadata::default();
    if left.safe.path != right.safe.path {
        return Ok(metadata);
    }
    let working_left = matches!(left.context.endpoint.reference, CompareRef::WorkingTree);
    let working_right = matches!(right.context.endpoint.reference, CompareRef::WorkingTree);
    if working_left || working_right {
        metadata.reason =
            Some("Working-tree rename metadata unavailable; clean filters are not executed".into());
        return Ok(metadata);
    }
    let mut args = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=all",
        "--find-renames",
        "-z",
    ];
    if working_left {
        args.extend(["-R", &right.commit]);
    } else {
        args.push(&left.commit);
        if !working_right {
            args.push(&right.commit);
        }
    }
    let mut names = args.clone();
    names.extend(["--name-status", "--"]);
    let output = job.output(&left.context.root, &names).await?;
    let fields: Vec<_> = output
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect();
    let mut index = 0;
    while index < fields.len() {
        let status = decode(fields[index])?;
        index += 1;
        let path = fields
            .get(index)
            .ok_or_else(|| Problem::new("gitError", "Invalid name-status record"))?;
        index += 1;
        if status.starts_with(['R', 'C']) {
            let to = decode(
                fields
                    .get(index)
                    .ok_or_else(|| Problem::new("gitError", "Invalid rename record"))?,
            )?;
            index += 1;
            let rename = Rename {
                from: decode(path)?,
                to: to.clone(),
                score: status[1..].into(),
            };
            metadata.renames.insert(rename.from.clone(), rename.clone());
            metadata.renames.insert(to, rename);
        }
    }
    let mut stats = args;
    stats.extend(["--numstat", "--"]);
    let output = job.output(&left.context.root, &stats).await?;
    let fields: Vec<_> = output.split(|byte| *byte == 0).collect();
    let mut index = 0;
    while index < fields.len() && !fields[index].is_empty() {
        let row: Vec<_> = fields[index].splitn(3, |byte| *byte == b'\t').collect();
        index += 1;
        if row.len() != 3 {
            return Err(Problem::new("gitError", "Invalid numstat record"));
        }
        let lines = if row[0] == b"-" || row[1] == b"-" {
            None
        } else {
            Some(Lines {
                added: decode(row[0])?
                    .parse()
                    .map_err(|_| Problem::new("gitError", "Invalid numstat count"))?,
                removed: decode(row[1])?
                    .parse()
                    .map_err(|_| Problem::new("gitError", "Invalid numstat count"))?,
            })
        };
        if row[2].is_empty() {
            index += 2;
            if index > fields.len() {
                return Err(Problem::new("gitError", "Invalid rename numstat record"));
            }
        } else {
            metadata.lines.insert(decode(row[2])?, lines);
        }
    }
    Ok(metadata)
}
