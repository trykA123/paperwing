use super::object_id::ObjectFormat;
use super::{decode, hex, CompareRef, Context, Job, Problem, UnavailableReason, FILE_LIMIT};
use crate::paths;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Kind {
    File,
    Symlink,
    Gitlink,
    Directory,
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) kind: Kind,
    pub(super) oid: Option<String>,
    pub(super) blob_id: Option<String>,
    pub(super) size: Option<u64>,
    pub(super) mode: String,
    pub(super) modified_ms: Option<u128>,
    pub(super) reason: Option<String>,
    pub(super) fingerprint: Option<u64>,
    pub(super) source: String,
}

fn add_folders(entries: &mut BTreeMap<String, Entry>) {
    for path in entries.keys().cloned().collect::<Vec<_>>() {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            entries.entry(prefix.into()).or_insert(Entry {
                kind: Kind::Directory,
                oid: None,
                blob_id: None,
                size: None,
                mode: "040000".into(),
                modified_ms: None,
                reason: None,
                fingerprint: None,
                source: "aggregate".into(),
            });
            parent = prefix;
        }
    }
}

pub(super) async fn inventory(resolved: &Resolved, job: &Job) -> Result<BTreeMap<String, Entry>, Problem> {
    let (context, commit) = (&resolved.context, resolved.commit.as_str());
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("compare.inventory", "other");
    let working = matches!(context.endpoint.reference, CompareRef::WorkingTree);
    let mut entries = BTreeMap::new();
    if !working {
        let output = job
            .output(&context.root, &["ls-tree", "-r", "-z", "--full-tree", "-l", commit, "--"])
            .await?;
        for record in output
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| Problem::new("gitError", "Invalid tree record"))?;
            let header = decode(&record[..tab])?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if fields.len() != 4 || !hex(fields[2]) {
                return Err(Problem::new("gitError", "Invalid tree metadata"));
            }
            let kind = match fields[0] {
                "120000" => Kind::Symlink,
                "160000" => Kind::Gitlink,
                "100644" | "100755" => Kind::File,
                _ => return Err(Problem::new("unavailable", "Unsupported tree mode")),
            };
            entries.insert(
                decode(&record[tab + 1..])?,
                Entry {
                    kind,
                    oid: Some(fields[2].into()),
                    blob_id: Some(fields[2].into()),
                    size: fields[3].parse().ok(),
                    mode: fields[0].into(),
                    modified_ms: None,
                    reason: None,
                    fingerprint: None,
                    source: "commitBlob".into(),
                },
            );
        }
    } else {
        let staged = job
            .output(&context.root, &["ls-files", "--stage", "-z", "--"])
            .await?;
        let mut index = HashMap::new();
        for record in staged
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| Problem::new("gitError", "Invalid index record"))?;
            let header = decode(&record[..tab])?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if fields.len() != 3 || fields[2] != "0" {
                return Err(Problem::unavailable(
                    UnavailableReason::UnmergedIndex,
                    "Unmerged index is unsupported",
                ));
            }
            index.insert(
                decode(&record[tab + 1..])?,
                (fields[0].to_string(), fields[1].to_string()),
            );
        }
        let output = job
            .output(
                &context.root,
                &[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "--",
                ],
            )
            .await?;
        let records: Vec<_> = output
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(decode)
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        if records.len() > FILE_LIMIT {
            return Err(Problem::new("limitExceeded", "Too many comparison files"));
        }
        super::index_objects::validate(context, &index, job).await?;
        entries =
            super::working_inventory::read(records, Arc::new(index), resolved, job).await?;
    }
    if entries.len() > FILE_LIMIT {
        return Err(Problem::new("limitExceeded", "Too many comparison files"));
    }
    add_folders(&mut entries);
    Ok(entries)
}

pub(super) fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone)]
pub(super) struct Resolved {
    pub(super) diff_config: Vec<String>,
    pub(super) context: Context,
    pub(super) safe: paths::ReadRoot,
    pub(super) commit: String,
    pub(super) files: BTreeMap<String, Entry>,
    pub(super) object_format: ObjectFormat,
    pub(super) reader: crate::git::BatchReader,
}

pub(super) async fn content(
    side: &Resolved,
    path: &str,
    entry: &Entry,
    job: &Job,
) -> Result<Vec<u8>, Problem> {
    job.check()?;
    if let Some(reason) = &entry.reason {
        return Err(Problem::new("unavailable", reason));
    }
    if entry.kind == Kind::Directory {
        return Err(Problem::new("unavailable", "Directory has no blob content"));
    }
    if entry.kind == Kind::Gitlink {
        return entry
            .oid
            .as_ref()
            .map(|oid| oid.as_bytes().to_vec())
            .ok_or_else(|| {
                Problem::new(
                    "unavailable",
                    "Untracked nested repository; contents are opaque",
                )
            });
    }
    if matches!(side.context.endpoint.reference, CompareRef::WorkingTree) {
        let safe = side.safe.clone();
        let path = path.to_string();
        let file = tokio::task::spawn_blocking(move || safe.read(&path))
            .await
            .map_err(|_| Problem::new("unavailable", "Content task failed"))?
            .map_err(|reason| Problem::new("unavailable", &reason))?
            .ok_or_else(|| {
                Problem::new(
                    "staleContent",
                    "Working-tree file was deleted; refresh required",
                )
            })?;
        if entry.fingerprint != Some(fingerprint(&file.bytes))
            || entry.size != Some(file.bytes.len() as u64)
            || entry.modified_ms != file.modified_ms
        {
            return Err(Problem::new(
                "staleContent",
                "Working-tree bytes changed; refresh required",
            ));
        }
        Ok(file.bytes)
    } else {
        if entry
            .size
            .is_none_or(|size| size > paths::CONTENT_LIMIT as u64)
        {
            return Err(Problem::new("unavailable", "Content exceeds read limit"));
        }
        let oid = entry
            .oid
            .as_ref()
            .ok_or_else(|| Problem::new("gitError", "Missing blob identity"))?;
        if !hex(oid) {
            return Err(Problem::new("gitError", "Invalid blob identity"));
        }
        let bytes = match side.reader.read(oid).await {
            Ok(bytes) => bytes,
            Err(_) => {
                job.check()?;
                job
            .output(&side.context.root, &["cat-file", "blob", oid])
            .await?
            }
        };
        if bytes.len() > paths::CONTENT_LIMIT {
            return Err(Problem::new("unavailable", "Content exceeds read limit"));
        }
        Ok(bytes)
    }
}
