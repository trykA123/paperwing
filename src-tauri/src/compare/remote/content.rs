use super::*;
use crate::compare::{Kind, Status};

pub(super) struct File {
    sha: Option<String>,
    size: Option<u64>,
    kind: Kind,
}

struct FileBlob {
    blob: Blob,
    kind: Kind,
}

pub(in crate::compare) struct ContentRequest<'a> {
    pub file_id: &'a str,
    pub side: &'a str,
    pub job: &'a Job,
}

impl Prepared {
    async fn file(&self, index: usize, side: usize) -> Result<&File, Problem> {
        self.blobs[index][side]
            .get_or_try_init(|| async {
                let file = &self.data.files[index];
                let absent = if side == 0 {
                    matches!(file.status.as_str(), "added" | "copied")
                } else {
                    file.status == "removed"
                };
                if absent {
                    return Ok(File {
                        sha: None,
                        size: Some(0),
                        kind: Kind::File,
                    });
                }
                let path = if side == 0 {
                    file.previous_filename.as_deref().unwrap_or(&file.filename)
                } else {
                    &file.filename
                };
                let commit = if side == 0 {
                    &self.data.merge_base
                } else {
                    &self.data.head
                };
                let reference =
                    crate::github::compare::blob_reference::file(&self.request, commit, path)
                        .await
                        .map_err(problem)?;
                let kind = match (reference.kind.as_str(), reference.mode.as_str()) {
                    ("blob", "100644" | "100755") => Kind::File,
                    ("blob", "120000") => Kind::Symlink,
                    ("commit", "160000") => Kind::Gitlink,
                    _ => {
                        return Err(Problem::new(
                            "githubUnavailable",
                            "Unsupported GitHub file mode",
                        ))
                    }
                };
                Ok(File {
                    sha: Some(reference.sha),
                    size: reference.size,
                    kind,
                })
            })
            .await
    }

    async fn blob(&self, cache: &Cache, index: usize, side: usize) -> Result<FileBlob, Problem> {
        let file = self.file(index, side).await?;
        let blob = match (&file.sha, &file.kind) {
            (_, Kind::Gitlink) => Blob {
                bytes: None,
                size: file.size,
                binary: false,
            },
            (Some(sha), _) => cache
                .read(&self.request, sha, file.size)
                .await
                .map_err(problem)?,
            (None, _) => Blob {
                bytes: Some(Vec::new()),
                size: Some(0),
                binary: false,
            },
        };
        Ok(FileBlob {
            blob,
            kind: file.kind.clone(),
        })
    }

    pub(in crate::compare) async fn content(
        &self,
        cache: &Cache,
        request: ContentRequest<'_>,
    ) -> Result<Vec<u8>, Problem> {
        request.job.check()?;
        let side = match request.side {
            "left" => 0,
            "right" => 1,
            _ => return Err(Problem::new("invalidContext", "Unknown side")),
        };
        let index = self
            .rows
            .lock()
            .await
            .iter()
            .position(|row| row.id == request.file_id)
            .ok_or_else(|| Problem::new("unknownFile", "Unknown file identity"))?;
        let file = &self.data.files[index];
        if (side == 0 && matches!(file.status.as_str(), "added" | "copied"))
            || (side == 1 && file.status == "removed")
        {
            return Err(Problem::new("unavailable", "File absent on selected side"));
        }
        let (left, right) =
            tokio::try_join!(self.blob(cache, index, 0), self.blob(cache, index, 1))?;
        request.job.check()?;
        self.update(index, [&left, &right], request.job).await?;
        let selected = if side == 0 { &left } else { &right };
        selected
            .blob
            .bytes
            .clone()
            .ok_or_else(|| Problem::new("unavailable", metadata_reason(selected)))
    }

    async fn update(&self, index: usize, sides: [&FileBlob; 2], job: &Job) -> Result<(), Problem> {
        let mut row = self.rows.lock().await[index].clone();
        let [left, right] = sides;
        for (side, blob) in [(&mut row.left, left), (&mut row.right, right)] {
            if let Some(side) = side {
                side.size = blob.blob.size;
                side.kind = blob.kind.clone();
                side.reason = blob
                    .blob
                    .bytes
                    .is_none()
                    .then(|| metadata_reason(blob).into());
            }
        }
        row.binary = Some(left.blob.binary || right.blob.binary);
        self.update_counts(&mut row, sides, job).await?;
        job.check()?;
        self.rows.lock().await[index] = row;
        Ok(())
    }
    async fn update_counts(
        &self,
        row: &mut crate::compare::FileRow,
        sides: [&FileBlob; 2],
        job: &Job,
    ) -> Result<(), Problem> {
        let [left, right] = sides;
        if let (Some(left), Some(right)) = (&left.blob.bytes, &right.blob.bytes) {
            row.raw_lines = super::super::count_result(
                counts::count([left.clone(), right.clone()], job).await,
                &mut row.reason,
            )?;
            row.display_lines =
                if self.view.options.normalize_eol || self.view.options.ignore_whitespace {
                    let left = super::super::normalized(left, &self.view.options);
                    let right = super::super::normalized(right, &self.view.options);
                    if left == right && row.raw_status == Status::Different {
                        row.display_status = Status::Same;
                    }
                    super::super::count_result(
                        counts::count([left, right], job).await,
                        &mut row.reason,
                    )?
                } else {
                    row.raw_lines.clone()
                };
        } else {
            row.raw_lines = None;
            row.display_lines = None;
            row.reason = Some("GitHub file content is metadata only".into());
        }
        Ok(())
    }
}

fn metadata_reason(blob: &FileBlob) -> &'static str {
    if blob.kind == Kind::Gitlink {
        "Submodule metadata only"
    } else if blob.blob.binary {
        "Binary file metadata only"
    } else {
        "File exceeds 5 MB; metadata only"
    }
}
