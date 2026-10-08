use super::*;
use crate::compare::{Kind, Lines, Rename, SideInfo, Status};

pub(super) fn map(data: &Comparison, options: &Options) -> Vec<FileRow> {
    data.files
        .iter()
        .map(|file| {
            let status = match file.status.as_str() {
                "added" | "copied" => Status::RightOnly,
                "removed" => Status::LeftOnly,
                "unchanged" => Status::Same,
                _ => Status::Different,
            };
            let lines = file
                .line_counts()
                .map(|(added, removed)| Lines { added, removed });
            let side = || SideInfo {
                kind: Kind::File,
                size: None,
                modified_ms: None,
                reason: None,
                source: "githubBlob".into(),
            };
            FileRow {
                id: format!("file-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
                path: file.filename.clone(),
                left: (status != Status::RightOnly).then(side),
                right: (status != Status::LeftOnly).then(side),
                raw_status: status.clone(),
                display_status: status,
                raw_lines: lines.clone(),
                display_lines: if options.normalize_eol || options.ignore_whitespace {
                    None
                } else {
                    lines
                },
                binary: None,
                rename: file
                    .previous_filename
                    .as_ref()
                    .filter(|_| file.status == "renamed")
                    .map(|name| Rename {
                        from: name.clone(),
                        to: file.filename.clone(),
                        score: "unknown".into(),
                    }),
                reason: None,
            }
        })
        .collect()
}

pub(super) fn snapshot(data: &Comparison, rows: &[FileRow], refresh: Refresh<'_>) -> Snapshot {
    let mut raw = Summary::default();
    let mut display = Summary::default();
    for row in rows {
        raw.add(&row.raw_status);
        display.add(&row.display_status);
    }
    let [left, right] = refresh.endpoints;
    let resolved = |endpoint, commit| ResolvedEndpoint {
        endpoint,
        commit,
        history_basis: "commit".into(),
    };
    Snapshot {
        id: refresh.id.into(),
        generation: refresh.generation,
        left: resolved(left, data.base.clone()),
        right: resolved(right, data.head.clone()),
        raw,
        display,
        history: history(data),
        file_count: rows.len(),
        options: refresh.options,
        source: "github",
        truncated: data.truncated.clone(),
    }
}

fn history(data: &Comparison) -> History {
    History {
        available: data.behind == 0,
        reason: (data.behind != 0).then(|| "GitHub compare lists only head-side commits".into()),
        left_count: Some(data.behind),
        right_count: Some(data.ahead),
        left_basis: "commit".into(),
        right_basis: "commit".into(),
    }
}
