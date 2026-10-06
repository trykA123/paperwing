use super::*;
#[path = "legacy/inventory.rs"]
mod inventory;
#[path = "legacy/text_diff.rs"]
pub(super) mod text_diff;
use inventory::{content, inventory};
use text_diff::{binary, count_result, diff_metadata, line_counts, normalized};
pub(super) async fn prepare(
    service: &Service,
    id: &str,
    generation: u64,
    contexts: [Context; 2],
    options: Options,
    job: &Job,
) -> Result<Prepared, Problem> {
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("compare.prepare", "other");
    let [left_context, right_context] = contexts;
    let left_safe = read_root(&left_context, job)
        .await
        .map_err(|problem| problem.side("left"))?;
    let right_safe = read_root(&right_context, job)
        .await
        .map_err(|problem| problem.side("right"))?;
    #[cfg(target_os = "linux")]
    let storage_job = {
        let mut captured = job.clone();
        if let Some(storage) = &captured.diff {
            captured.roots = storage
                .capture(
                    vec![left_safe.clone(), right_safe.clone()],
                    &captured.cancel,
                )
                .await
                .map_err(|error| {
                    Problem::new(
                        if error.cancelled {
                            "cancelled"
                        } else {
                            "unavailable"
                        },
                        error.message,
                    )
                })?;
        }
        captured
    };
    #[cfg(target_os = "linux")]
    let job = &storage_job;
    let contexts = [&left_context, &right_context];
    let roots = [&left_safe.path, &right_safe.path];
    let mut states = Vec::new();
    let mut epochs = Vec::new();
    for root in roots {
        let state = service.fetch_state(root).await?;
        epochs.push(job.lock(&state).await?.epoch);
        states.push(state);
    }
    let mut commits = Vec::new();
    for (index, context) in contexts.iter().enumerate() {
        commits.push(
            resolve(context, job)
                .await
                .map_err(|problem| problem.side(if index == 0 { "left" } else { "right" }))?,
        );
    }
    let mut attempted = BTreeSet::new();
    for index in 0..2 {
        if commits[index].is_some() {
            continue;
        }
        let side = if index == 0 { "left" } else { "right" };
        let mut state = job.lock(&states[index]).await?;
        if state.epoch == epochs[index]
            && attempted.insert(roots[index].clone())
            && resolve(contexts[index], job).await?.is_none()
        {
            let result = job
                .run(
                    &contexts[index].root,
                    &[
                        "-c",
                        "gc.auto=0",
                        "-c",
                        "maintenance.auto=false",
                        "-c",
                        "protocol.ext.allow=never",
                        "-c",
                        "fetch.prune=false",
                        "-c",
                        "fetch.pruneTags=false",
                        "-c",
                        "remote.origin.prune=false",
                        "-c",
                        "remote.origin.pruneTags=false",
                        "fetch",
                        "--no-prune",
                        "--no-write-fetch-head",
                        "--no-auto-maintenance",
                        "--no-recurse-submodules",
                        "--",
                        "origin",
                    ],
                    &[0],
                )
                .await;
            state.epoch += 1;
            state.problem = match result {
                Ok(result) if result.code == Some(0) => None,
                Ok(result) => Some(Problem::new("networkError", &result.last_error())),
                Err(problem) if problem.kind == "cancelled" => return Err(problem),
                Err(problem) => Some(Problem::new("networkError", &problem.message)),
            };
        }
        commits[index] = resolve(contexts[index], job)
            .await
            .map_err(|problem| problem.side(side))?;
        if commits[index].is_none() {
            if let Some(problem) = &state.problem {
                return Err(problem.clone().side(side));
            }
            return Err(Problem::new(
                if index == 0 {
                    "missingLeft"
                } else {
                    "missingRight"
                },
                "Reference is missing after one origin fetch",
            )
            .side(side));
        }
    }
    let mut left = Resolved {
        diff_config: Vec::new(),
        object_format: ObjectFormat::Sha1,
        reader: git::BatchReader::new(left_context.root.clone(), job.cancel.clone()),
        context: left_context,
        safe: left_safe,
        commit: commits[0].take().unwrap(),
        files: BTreeMap::new(),
    };
    let mut right = Resolved {
        diff_config: Vec::new(),
        object_format: ObjectFormat::Sha1,
        reader: git::BatchReader::new(right_context.root.clone(), job.cancel.clone()),
        context: right_context,
        safe: right_safe,
        commit: commits[1].take().unwrap(),
        files: BTreeMap::new(),
    };
    left.files = inventory(&left.context, &left.safe, &left.commit, job)
        .await
        .map_err(|problem| problem.side("left"))?;
    right.files = inventory(&right.context, &right.safe, &right.commit, job)
        .await
        .map_err(|problem| problem.side("right"))?;
    let metadata = diff_metadata(&left, &right, job).await?;
    let paths: BTreeSet<_> = left
        .files
        .keys()
        .chain(right.files.keys())
        .cloned()
        .collect();
    if paths.len() > FILE_LIMIT {
        return Err(Problem::new(
            "limitExceeded",
            "Comparison exceeds inventory limit",
        ));
    }
    let mut rows = Vec::new();
    let mut used = 0;
    for path in paths {
        job.check()?;
        let left_entry = left.files.get(&path);
        let right_entry = right.files.get(&path);
        let raw_status = match (left_entry, right_entry) {
            (left, right)
                if [left, right]
                    .into_iter()
                    .flatten()
                    .any(|entry| entry.reason.is_some()) =>
            {
                Status::Unavailable
            }
            (Some(left), Some(right)) if left.kind != right.kind => Status::TypeConflict,
            (Some(_), None) => Status::LeftOnly,
            (None, Some(_)) => Status::RightOnly,
            _ => Status::Same,
        };
        let mut row = FileRow {
            id: format!("file-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
            path: path.clone(),
            left: left_entry.map(SideInfo::from),
            right: right_entry.map(SideInfo::from),
            display_status: raw_status.clone(),
            raw_status,
            raw_lines: None,
            display_lines: None,
            binary: None,
            rename: metadata.renames.get(&path).cloned(),
            reason: metadata.reason.clone(),
        };
        let leaves = [left_entry, right_entry]
            .into_iter()
            .flatten()
            .all(|entry| entry.kind != Kind::Directory);
        if [left_entry, right_entry]
            .into_iter()
            .flatten()
            .any(|entry| entry.source == "untrackedRepository")
        {
            row.reason = Some("Untracked nested repository; contents are opaque".into());
            if left_entry.is_some() && right_entry.is_some() && row.raw_status == Status::Same {
                row.raw_status = Status::Unavailable;
                row.display_status = Status::Unavailable;
            }
            rows.push(row);
            continue;
        }
        if leaves && row.raw_status != Status::TypeConflict {
            let identical_oid = matches!((left_entry, right_entry), (Some(left), Some(right)) if left.reason.is_none() && right.reason.is_none() && left.oid.is_some() && left.oid == right.oid && left.mode == right.mode)
                && left.safe.path == right.safe.path;
            if !identical_oid {
                let cost = [left_entry, right_entry]
                    .into_iter()
                    .flatten()
                    .map(|entry| entry.size.unwrap_or(0) as usize)
                    .sum::<usize>();
                if cost > BYTE_LIMIT.saturating_sub(used) {
                    row.reason = Some("Comparison diff content budget exceeded".into());
                    row.raw_status = Status::Unavailable;
                    row.display_status = Status::Unavailable;
                    rows.push(row);
                    continue;
                }
                let mut bytes = Vec::new();
                for (side, entry) in [(&left, left_entry), (&right, right_entry)] {
                    match entry {
                        Some(entry) => match content(side, &path, entry, job).await {
                            Ok(content) => {
                                used += content.len();
                                bytes.push(content);
                            }
                            Err(problem) if problem.kind == "cancelled" => return Err(problem),
                            Err(problem) => {
                                row.reason = Some(problem.message);
                                row.raw_status = Status::Unavailable;
                                row.display_status = Status::Unavailable;
                                break;
                            }
                        },
                        None => bytes.push(Vec::new()),
                    }
                }
                if used > BYTE_LIMIT {
                    row.reason = Some("Comparison diff content budget exceeded".into());
                    row.raw_status = Status::Unavailable;
                    row.display_status = Status::Unavailable;
                    rows.push(row);
                    continue;
                }
                if bytes.len() == 2 {
                    let opaque = [left_entry, right_entry]
                        .into_iter()
                        .flatten()
                        .any(|entry| entry.kind == Kind::Gitlink);
                    let is_binary = binary(&bytes[0]) || binary(&bytes[1]);
                    row.binary = Some(is_binary);
                    if let (Some(left), Some(right)) = (left_entry, right_entry) {
                        let mode_same = left.mode == right.mode;
                        row.raw_status = if bytes[0] == bytes[1] && mode_same {
                            Status::Same
                        } else {
                            Status::Different
                        };
                        row.display_status = if normalized(&bytes[0], &options)
                            == normalized(&bytes[1], &options)
                            && mode_same
                        {
                            Status::Same
                        } else {
                            Status::Different
                        };
                    }
                    if !opaque && !is_binary {
                        row.raw_lines = if row.raw_status == Status::Same {
                            Some(Lines {
                                added: 0,
                                removed: 0,
                            })
                        } else if !matches!(
                            left.context.endpoint.reference,
                            CompareRef::WorkingTree
                        ) && !matches!(
                            right.context.endpoint.reference,
                            CompareRef::WorkingTree
                        ) && metadata.lines.contains_key(&path)
                        {
                            metadata.lines[&path].clone()
                        } else {
                            count_result(
                                line_counts(&bytes[0], &bytes[1], job).await,
                                &mut row.reason,
                            )?
                        };
                        row.display_lines = if row.display_status == Status::Same {
                            Some(Lines {
                                added: 0,
                                removed: 0,
                            })
                        } else if options.normalize_eol || options.ignore_whitespace {
                            count_result(
                                line_counts(
                                    &normalized(&bytes[0], &options),
                                    &normalized(&bytes[1], &options),
                                    job,
                                )
                                .await,
                                &mut row.reason,
                            )?
                        } else {
                            row.raw_lines.clone()
                        };
                    }
                }
            }
        }
        rows.push(row);
    }
    for index in (0..rows.len()).rev() {
        if ![rows[index].left.as_ref(), rows[index].right.as_ref()]
            .into_iter()
            .flatten()
            .any(|side| side.kind == Kind::Directory)
        {
            continue;
        }
        let prefix = format!("{}/", rows[index].path);
        let children: Vec<_> = rows
            .iter()
            .filter(|row| row.path.starts_with(&prefix) && !row.path[prefix.len()..].contains('/'))
            .cloned()
            .collect();
        if rows[index].raw_status == Status::Same {
            rows[index].raw_status = if children
                .iter()
                .any(|row| row.raw_status == Status::Unavailable)
            {
                Status::Unavailable
            } else if children.iter().all(|row| row.raw_status == Status::Same) {
                Status::Same
            } else {
                Status::Different
            };
            rows[index].display_status = if children
                .iter()
                .any(|row| row.display_status == Status::Unavailable)
            {
                Status::Unavailable
            } else if children
                .iter()
                .all(|row| row.display_status == Status::Same)
            {
                Status::Same
            } else {
                Status::Different
            };
        }
        for is_left in [true, false] {
            let info = if is_left {
                &mut rows[index].left
            } else {
                &mut rows[index].right
            };
            if let Some(info) = info.as_mut().filter(|side| side.kind == Kind::Directory) {
                let sides: Vec<_> = children
                    .iter()
                    .filter_map(|row| {
                        if is_left {
                            row.left.as_ref()
                        } else {
                            row.right.as_ref()
                        }
                    })
                    .collect();
                info.size = sides.iter().try_fold(0u64, |sum, side| {
                    side.size.and_then(|size| sum.checked_add(size))
                });
                info.modified_ms = sides.iter().filter_map(|side| side.modified_ms).max();
            }
        }
    }
    let mut raw = Summary::default();
    let mut display = Summary::default();
    for row in &rows {
        if [row.left.as_ref(), row.right.as_ref()]
            .into_iter()
            .flatten()
            .any(|side| side.kind != Kind::Directory)
        {
            raw.add(&row.raw_status);
            display.add(&row.display_status);
        }
    }
    let (history, history_source) = history(&left, &right, job).await?;
    let endpoint = |side: &Resolved, basis: String| ResolvedEndpoint {
        endpoint: side.context.endpoint.clone(),
        commit: side.commit.clone(),
        history_basis: basis,
    };
    let view = Snapshot {
        id: id.into(),
        generation,
        left: endpoint(&left, history.left_basis.clone()),
        right: endpoint(&right, history.right_basis.clone()),
        raw,
        display,
        history,
        file_count: rows.len(),
        options,
    };
    Ok(Prepared {
        view,
        left,
        right,
        rows,
        history_source,
    })
}

pub(super) async fn counts(left: &[u8], right: &[u8], job: &Job) -> Result<Option<Lines>, Problem> {
    text_diff::line_counts(left, right, job).await
}

pub(super) async fn bytes(
    side: &Resolved,
    path: &str,
    entry: &Entry,
    job: &Job,
) -> Result<Vec<u8>, Problem> {
    inventory::content(side, path, entry, job).await
}

#[cfg(target_os = "linux")]
pub(super) async fn metadata(
    left: &Resolved,
    right: &Resolved,
    job: &Job,
) -> Result<super::super::text_diff::DiffMetadata, Problem> {
    let old = text_diff::diff_metadata(left, right, job).await?;
    Ok(super::super::text_diff::DiffMetadata {
        lines: old.lines,
        renames: old.renames,
        reason: old.reason,
    })
}
