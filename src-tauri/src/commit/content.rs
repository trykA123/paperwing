use super::run;
use crate::git::OutputPolicy;
use std::time::Duration;

pub(super) async fn refuse_filters(path: &str, file: &str) -> Result<(), String> {
    let output = run(
        path,
        &["check-attr", "-z", "filter", "--", file],
        "Check partial staging filters",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
    match fields.as_slice() {
        [_, b"filter", b"unspecified" | b"unset", b""] => Ok(()),
        [_, b"filter", _, b""] => {
            Err("Files with a filter attribute do not support hunk staging or discard".into())
        }
        _ => Err("Could not read the file's filter attribute".into()),
    }
}

async fn hash(path: &str, file: &str, bytes: &[u8], clean: bool) -> Result<String, String> {
    let file_arg = format!("--path={file}");
    let conversion = if clean {
        file_arg.as_str()
    } else {
        "--no-filters"
    };
    let output = run(
        path,
        &["hash-object", "-w", "--stdin", conversion],
        "Convert partial staging content",
        &[0],
        OutputPolicy::Metadata,
        Some(bytes),
        Duration::from_secs(45),
    )
    .await?;
    let oid = std::str::from_utf8(&output.stdout)
        .map_err(|_| "Invalid converted blob id")?
        .trim();
    if !crate::object_id::valid(oid) {
        return Err("Invalid converted blob id".into());
    }
    Ok(oid.into())
}

pub(super) async fn clean(path: &str, file: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let oid = hash(path, file, bytes, true).await?;
    super::blob(path, &oid)
        .await?
        .ok_or_else(|| "Converted working content is missing".into())
}

pub(super) async fn smudge(path: &str, file: &str, spec: &str) -> Result<Vec<u8>, String> {
    let file_arg = format!("--path={file}");
    let output = run(
        path,
        &["cat-file", "--filters", &file_arg, spec],
        "Restore working-tree content",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    if output.stdout.len() > crate::paths::CONTENT_LIMIT {
        return Err("The restored file is too large".into());
    }
    Ok(output.stdout)
}

pub(super) async fn smudge_bytes(path: &str, file: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let oid = hash(path, file, bytes, false).await?;
    smudge(path, file, &oid).await
}
