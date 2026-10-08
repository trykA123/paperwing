use super::*;
use text_diff::DiffMetadata;

pub(super) struct Classifier<'a> {
    pub left: &'a Resolved,
    pub right: &'a Resolved,
    pub options: &'a Options,
    pub metadata: &'a DiffMetadata,
    pub job: &'a Job,
}

pub(super) fn status(left: Option<&Entry>, right: Option<&Entry>) -> Status {
    match (left, right) {
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
    }
}

pub(super) fn identical(left: &Resolved, right: &Resolved, path: &str) -> bool {
    matches!((left.files.get(path), right.files.get(path)), (Some(left), Some(right))
        if left.reason.is_none() && right.reason.is_none() && left.oid.is_some()
        && left.blob_id == right.blob_id && left.oid == right.oid && left.mode == right.mode)
        && left.safe.path == right.safe.path
}

pub(super) fn folder(row: &FileRow) -> bool {
    [row.left.as_ref(), row.right.as_ref()]
        .into_iter()
        .flatten()
        .any(|side| side.kind == Kind::Directory)
}

pub(super) fn counted(row: &FileRow) -> bool {
    [row.left.as_ref(), row.right.as_ref()]
        .into_iter()
        .flatten()
        .any(|side| side.kind != Kind::Directory)
}

impl Classifier<'_> {
    pub fn row(&self, pending: &PendingRow) -> FileRow {
        let status = status(
            self.left.files.get(&pending.path),
            self.right.files.get(&pending.path),
        );
        FileRow {
            id: pending.id.clone(),
            path: pending.path.clone(),
            left: pending.left.clone(),
            right: pending.right.clone(),
            raw_status: status.clone(),
            display_status: status,
            raw_lines: None,
            display_lines: None,
            binary: None,
            rename: self.metadata.renames.get(&pending.path).cloned(),
            reason: self.metadata.reason.clone(),
        }
    }

    pub fn fixed(&self, row: &mut FileRow) -> bool {
        let entries = [
            self.left.files.get(&row.path),
            self.right.files.get(&row.path),
        ];
        if entries
            .into_iter()
            .flatten()
            .any(|entry| entry.source == "untrackedRepository")
        {
            row.reason = Some("Untracked nested repository; contents are opaque".into());
            if row.left.is_some() && row.right.is_some() && row.raw_status == Status::Same {
                row.raw_status = Status::Unavailable;
                row.display_status = Status::Unavailable;
            }
            return true;
        }
        !folder(row)
            && (row.raw_status == Status::TypeConflict
                || identical(self.left, self.right, &row.path))
    }

    pub async fn commit(&self, mut row: FileRow, used: &mut usize) -> Result<FileRow, Problem> {
        self.job.check()?;
        let cost = [
            self.left.files.get(&row.path),
            self.right.files.get(&row.path),
        ]
        .into_iter()
        .flatten()
        .map(|entry| entry.size.unwrap_or(0) as usize)
        .sum::<usize>();
        if cost > BYTE_LIMIT.saturating_sub(*used) {
            return Ok(budget_row(row));
        }
        let _flight = flight::acquire(self.job, true, flight::ROW_BYTES).await?;
        let bytes = self.read(&mut row, used).await?;
        if *used > BYTE_LIMIT {
            return Ok(budget_row(row));
        }
        if bytes.len() == 2 {
            self.classify(&mut row, &bytes).await?;
        }
        Ok(row)
    }

    async fn read(&self, row: &mut FileRow, used: &mut usize) -> Result<Vec<Vec<u8>>, Problem> {
        let mut bytes = Vec::new();
        for side in [self.left, self.right] {
            let Some(entry) = side.files.get(&row.path) else {
                bytes.push(Vec::new());
                continue;
            };
            match content(side, &row.path, entry, self.job).await {
                Ok(content) => {
                    *used += content.len();
                    bytes.push(content);
                }
                Err(problem) if problem.kind == "cancelled" => return Err(problem),
                Err(problem) => {
                    row.reason = Some(problem.message);
                    row.raw_status = Status::Unavailable;
                    row.display_status = Status::Unavailable;
                    break;
                }
            }
        }
        Ok(bytes)
    }

    async fn classify(&self, row: &mut FileRow, bytes: &[Vec<u8>]) -> Result<(), Problem> {
        let left = self.left.files.get(&row.path);
        let right = self.right.files.get(&row.path);
        let opaque = [left, right]
            .into_iter()
            .flatten()
            .any(|entry| entry.kind == Kind::Gitlink);
        let is_binary = binary(&bytes[0]) || binary(&bytes[1]);
        row.binary = Some(is_binary);
        if let (Some(left), Some(right)) = (left, right) {
            row.raw_status = if bytes[0] == bytes[1] && left.mode == right.mode {
                Status::Same
            } else {
                Status::Different
            };
            row.display_status = if normalized(&bytes[0], self.options)
                == normalized(&bytes[1], self.options)
                && left.mode == right.mode
            {
                Status::Same
            } else {
                Status::Different
            };
        }
        if !opaque && !is_binary {
            self.count(row, bytes).await?;
        }
        Ok(())
    }

    async fn count(&self, row: &mut FileRow, bytes: &[Vec<u8>]) -> Result<(), Problem> {
        row.raw_lines = if row.raw_status == Status::Same {
            Some(Lines {
                added: 0,
                removed: 0,
            })
        } else if !matches!(
            self.left.context.endpoint.reference,
            CompareRef::WorkingTree
        ) && !matches!(
            self.right.context.endpoint.reference,
            CompareRef::WorkingTree
        ) && self.metadata.lines.contains_key(&row.path)
        {
            self.metadata.lines[&row.path].clone()
        } else {
            count_result(
                line_counts(&bytes[0], &bytes[1], self.job).await,
                &mut row.reason,
            )?
        };
        row.display_lines = if row.display_status == Status::Same {
            Some(Lines {
                added: 0,
                removed: 0,
            })
        } else if self.options.normalize_eol || self.options.ignore_whitespace {
            count_result(
                line_counts(
                    &normalized(&bytes[0], self.options),
                    &normalized(&bytes[1], self.options),
                    self.job,
                )
                .await,
                &mut row.reason,
            )?
        } else {
            row.raw_lines.clone()
        };
        Ok(())
    }
}

fn budget_row(mut row: FileRow) -> FileRow {
    row.reason = Some("Comparison diff content budget exceeded".into());
    row.raw_status = Status::Unavailable;
    row.display_status = Status::Unavailable;
    row
}
