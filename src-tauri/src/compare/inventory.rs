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
    pub(super) size: Option<u64>,
    pub(super) mode: String,
    pub(super) modified_ms: Option<u128>,
    pub(super) reason: Option<String>,
    fingerprint: Option<u64>,
    pub(super) source: String,
}

fn add_folders(entries: &mut BTreeMap<String, Entry>) {
    for path in entries.keys().cloned().collect::<Vec<_>>() {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            entries.entry(prefix.into()).or_insert(Entry {
                kind: Kind::Directory,
                oid: None,
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

async fn index_sizes(
    context: &Context,
    index: &HashMap<String, (String, String)>,
    job: &Job,
) -> Result<HashMap<String, usize>, Problem> {
    let objects: BTreeSet<_> = index
        .values()
        .filter(|(mode, _)| mode != "160000")
        .map(|(_, oid)| oid.clone())
        .collect();
    let objects: Vec<_> = objects.into_iter().collect();
    let mut sizes = HashMap::new();
    for chunk in objects.chunks(1024) {
        let input = format!("{}\n", chunk.join("\n"));
        let result = job
            .run_input(
                &context.root,
                &["cat-file", "--batch-check"],
                &[0],
                Some(input.as_bytes()),
            )
            .await?;
        if result.code != Some(0) {
            return Err(Problem::new("gitError", "Index object inspection failed"));
        }
        for line in decode(&result.stdout)?.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 3 || fields[1] != "blob" || !hex(fields[0]) {
                return Err(Problem::new("gitError", "Invalid index object metadata"));
            }
            sizes.insert(
                fields[0].into(),
                fields[2]
                    .parse()
                    .map_err(|_| Problem::new("gitError", "Invalid blob size"))?,
            );
        }
    }
    Ok(sizes)
}

async fn index_blobs(
    context: &Context,
    objects: BTreeSet<String>,
    job: &Job,
) -> Result<HashMap<String, Vec<u8>>, Problem> {
    if objects.is_empty() {
        return Ok(HashMap::new());
    }
    let input = format!(
        "{}\n",
        objects.iter().cloned().collect::<Vec<_>>().join("\n")
    );
    let result = job
        .run_input(
            &context.root,
            &["cat-file", "--batch"],
            &[0],
            Some(input.as_bytes()),
        )
        .await?;
    if result.code != Some(0) {
        return Err(Problem::new("gitError", "Index blob batch failed"));
    }
    let mut output = result.stdout.as_slice();
    let mut blobs = HashMap::new();
    for object in objects {
        let end = output
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| Problem::new("gitError", "Invalid batch header"))?;
        let header = decode(&output[..end])?;
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[0] != object || fields[1] != "blob" {
            return Err(Problem::new("gitError", "Unexpected batch object"));
        }
        let size: usize = fields[2]
            .parse()
            .map_err(|_| Problem::new("gitError", "Invalid batch size"))?;
        output = &output[end + 1..];
        if size > paths::CONTENT_LIMIT || output.get(size) != Some(&b'\n') {
            return Err(Problem::new("gitError", "Invalid batch content"));
        }
        blobs.insert(object, output[..size].to_vec());
        output = &output[size + 1..];
    }
    if !output.is_empty() {
        return Err(Problem::new("gitError", "Unexpected batch tail"));
    }
    Ok(blobs)
}

pub(super) async fn inventory(
    context: &Context,
    safe: &paths::ReadRoot,
    commit: &str,
    job: &Job,
) -> Result<BTreeMap<String, Entry>, Problem> {
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("compare.inventory", "other");
    let working = matches!(context.endpoint.reference, CompareRef::WorkingTree);
    let mut entries = BTreeMap::new();
    if !working {
        let output = job
            .output(&context.root, &["ls-tree", "-r", "-z", "-l", commit, "--"])
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
        let sizes = index_sizes(context, &index, job).await?;
        let index = Arc::new(index);
        let mut cache = paths::ReadCache::default();
        let mut start = 0;
        while start < records.len() {
            job.check()?;
            let mut end = start;
            let mut size = 0;
            let mut objects = BTreeSet::new();
            while end < records.len() && end - start < 512 {
                let object = index.get(&records[end]).map(|(_, oid)| oid);
                let bytes = object
                    .and_then(|oid| sizes.get(oid))
                    .copied()
                    .filter(|size| *size <= paths::CONTENT_LIMIT)
                    .unwrap_or(0);
                if size + bytes > 4 * 1024 * 1024 {
                    break;
                }
                size += bytes;
                if let Some(oid) = object.filter(|oid| {
                    sizes
                        .get(*oid)
                        .is_some_and(|size| *size <= paths::CONTENT_LIMIT)
                }) {
                    objects.insert(oid.clone());
                }
                end += 1;
            }
            let blobs = index_blobs(context, objects, job).await?;
            let records = records[start..end].to_vec();
            let safe = safe.clone();
            let index = index.clone();
            let job = job.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<_, Problem> {
                let mut entries = BTreeMap::new();
                for path in records {
                    job.check()?;
                    if let Some(directory) = path.strip_suffix('/') {
                        let reason = safe.resolve_cached(directory, false, &mut cache).err();
                        entries.insert(
                            directory.into(),
                            Entry {
                                kind: Kind::Gitlink,
                                oid: None,
                                size: None,
                                mode: "160000".into(),
                                modified_ms: None,
                                reason,
                                fingerprint: None,
                                source: "untrackedRepository".into(),
                            },
                        );
                        continue;
                    }
                    if entries.contains_key(&path) {
                        continue;
                    }
                    if index.get(&path).is_some_and(|(mode, _)| mode == "160000") {
                        let resolved = safe.resolve_cached(&path, false, &mut cache);
                        let reason = resolved.as_ref().err().cloned();
                        if !resolved.is_ok_and(|path| path.exists()) && reason.is_none() {
                            continue;
                        }
                        entries.insert(
                            path.clone(),
                            Entry {
                                kind: Kind::Gitlink,
                                oid: Some(index[&path].1.clone()),
                                size: None,
                                mode: "160000".into(),
                                modified_ms: None,
                                reason,
                                fingerprint: None,
                                source: "indexGitlink".into(),
                            },
                        );
                        continue;
                    }
                    let result = safe.read_cached(&path, &mut cache);
                    #[cfg(test)]
                    if let Some(started) = &job.inventory_started {
                        started.notify_one();
                    }
                    match result {
                        Ok(Some(file)) => {
                            let link = file.symlink
                                || index.get(&path).is_some_and(|(mode, _)| mode == "120000");
                            let mode = if link {
                                "120000".into()
                            } else {
                                index
                                    .get(&path)
                                    .map(|(mode, _)| mode.clone())
                                    .unwrap_or_else(|| "100644".into())
                            };
                            let oid = index.get(&path).and_then(|(_, oid)| {
                                blobs
                                    .get(oid)
                                    .filter(|bytes| **bytes == file.bytes)
                                    .map(|_| oid.clone())
                            });
                            entries.insert(
                                path,
                                Entry {
                                    kind: if link { Kind::Symlink } else { Kind::File },
                                    oid,
                                    size: Some(file.bytes.len() as u64),
                                    mode,
                                    modified_ms: file.modified_ms,
                                    reason: None,
                                    fingerprint: Some(fingerprint(&file.bytes)),
                                    source: "workingTree".into(),
                                },
                            );
                        }
                        Ok(None) => {}
                        Err(reason) => {
                            entries.insert(
                                path,
                                Entry {
                                    kind: Kind::File,
                                    oid: None,
                                    size: None,
                                    mode: "100644".into(),
                                    modified_ms: None,
                                    reason: Some(reason),
                                    fingerprint: None,
                                    source: "workingTree".into(),
                                },
                            );
                        }
                    }
                }
                Ok((entries, cache))
            })
            .await
            .map_err(|_| Problem::new("unavailable", "Working inventory task failed"))??;
            entries.extend(result.0);
            cache = result.1;
            start = end;
        }
    }
    if entries.len() > FILE_LIMIT {
        return Err(Problem::new("limitExceeded", "Too many comparison files"));
    }
    add_folders(&mut entries);
    Ok(entries)
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone)]
pub(super) struct Resolved {
    pub(super) context: Context,
    pub(super) safe: paths::ReadRoot,
    pub(super) commit: String,
    pub(super) files: BTreeMap<String, Entry>,
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
        let bytes = job
            .output(&side.context.root, &["cat-file", "blob", oid])
            .await?;
        if bytes.len() > paths::CONTENT_LIMIT {
            return Err(Problem::new("unavailable", "Content exceeds read limit"));
        }
        Ok(bytes)
    }
}

