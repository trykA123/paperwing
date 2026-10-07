mod lines;
mod render;
use lines::diff_lines;
use render::render;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Context,
    Add,
    Remove,
}

#[derive(Clone)]
struct Edit<'a> {
    kind: Kind,
    bytes: &'a [u8],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    kind: &'static str,
    text: String,
    no_newline: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub index: usize,
    old_start: usize,
    new_start: usize,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub hunk: usize,
    pub ranges: Option<Vec<LineRange>>,
}

pub(crate) struct Diff<'a> {
    edits: Vec<Edit<'a>>,
    spans: Vec<std::ops::Range<usize>>,
}

pub(crate) struct Built {
    pub patch: Vec<u8>,
    pub content: Option<Vec<u8>>,
}

pub(crate) struct Paths<'a> {
    pub old: &'a str,
    pub new: &'a str,
    pub before: bool,
    pub after: bool,
    pub mode: &'a str,
}

pub(crate) fn binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

fn spans(edits: &[Edit<'_>]) -> Vec<std::ops::Range<usize>> {
    let mut spans: Vec<std::ops::Range<usize>> = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        if edit.kind == Kind::Context {
            continue;
        }
        let span = index.saturating_sub(3)..(index + 4).min(edits.len());
        if let Some(previous) = spans
            .last_mut()
            .filter(|previous| previous.end >= span.start)
        {
            previous.end = span.end;
        } else {
            spans.push(span);
        }
    }
    spans
}

fn validate_newlines(lines: &[Edit<'_>]) -> Result<(), String> {
    for excluded in [Kind::Add, Kind::Remove] {
        let mut missing_newline = false;
        for line in lines.iter().filter(|line| line.kind != excluded) {
            if missing_newline {
                return Err(
                    "Select the last line's change too: the file has no final newline".into(),
                );
            }
            missing_newline = !line.bytes.ends_with(b"\n");
        }
    }
    Ok(())
}

impl<'a> Diff<'a> {
    pub fn new(before: &'a [u8], after: &'a [u8]) -> Result<Self, String> {
        if binary(before) || binary(after) {
            return Err("Binary files do not support hunk or line actions".into());
        }
        let old: Vec<_> = before.split_inclusive(|byte| *byte == b'\n').collect();
        let new: Vec<_> = after.split_inclusive(|byte| *byte == b'\n').collect();
        let edits = diff_lines(&old, &new)?;
        let spans = spans(&edits);
        Ok(Self { edits, spans })
    }

    pub fn hunks(&self) -> Vec<Hunk> {
        self.spans
            .iter()
            .enumerate()
            .map(|(index, span)| Hunk {
                index,
                old_start: self.edits[..span.start]
                    .iter()
                    .filter(|edit| edit.kind != Kind::Add)
                    .count()
                    + 1,
                new_start: self.edits[..span.start]
                    .iter()
                    .filter(|edit| edit.kind != Kind::Remove)
                    .count()
                    + 1,
                lines: self.edits[span.clone()]
                    .iter()
                    .map(|edit| Line {
                        kind: match edit.kind {
                            Kind::Context => "context",
                            Kind::Add => "add",
                            Kind::Remove => "remove",
                        },
                        text: String::from_utf8_lossy(
                            edit.bytes.strip_suffix(b"\n").unwrap_or(edit.bytes),
                        )
                        .into_owned(),
                        no_newline: !edit.bytes.ends_with(b"\n"),
                    })
                    .collect(),
            })
            .collect()
    }

    fn selected(&self, selections: &[Selection]) -> Result<BTreeSet<usize>, String> {
        if selections.is_empty() || selections.len() > self.spans.len() {
            return Err("Select at least one changed hunk or line".into());
        }
        let mut selected = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for selection in selections {
            let span = self
                .spans
                .get(selection.hunk)
                .ok_or("Unknown hunk; refresh the diff")?;
            if !seen.insert(selection.hunk) {
                return Err("Duplicate hunk selection".into());
            }
            let ranges = selection.ranges.clone().unwrap_or_else(|| {
                vec![LineRange {
                    start: 0,
                    end: span.len() - 1,
                }]
            });
            if ranges.is_empty() || ranges.len() > span.len() {
                return Err("Select at least one line range".into());
            }
            for range in ranges {
                if range.start > range.end || range.end >= span.len() {
                    return Err("Line range is outside its hunk".into());
                }
                selected.extend(
                    (range.start..=range.end)
                        .map(|line| span.start + line)
                        .filter(|index| self.edits[*index].kind != Kind::Context),
                );
            }
        }
        if selected.is_empty() {
            return Err("Select changed lines, not only context".into());
        }
        Ok(selected)
    }

    pub fn build(
        &self,
        paths: Paths<'_>,
        selections: &[Selection],
        reverse: bool,
    ) -> Result<Built, String> {
        let selected = self.selected(selections)?;
        let mut lines = Vec::new();
        let mut content = Vec::new();
        for (index, edit) in self.edits.iter().enumerate() {
            let kind = if selected.contains(&index) {
                Some(edit.kind)
            } else {
                match (reverse, edit.kind) {
                    (_, Kind::Context) | (false, Kind::Remove) | (true, Kind::Add) => {
                        Some(Kind::Context)
                    }
                    _ => None,
                }
            };
            let Some(kind) = kind else { continue };
            if kind == Kind::Context || kind == if reverse { Kind::Remove } else { Kind::Add } {
                content.extend_from_slice(edit.bytes);
            }
            lines.push(Edit {
                kind,
                bytes: edit.bytes,
            });
        }
        validate_newlines(&lines)?;
        let all = self
            .edits
            .iter()
            .filter(|edit| edit.kind != Kind::Context)
            .count()
            == selected.len();
        let exists = !all || if reverse { paths.before } else { paths.after };
        let (before, after) = if reverse {
            (exists, paths.after)
        } else {
            (paths.before, exists)
        };
        let paths = Paths {
            old: if reverse && !all {
                paths.new
            } else {
                paths.old
            },
            ..paths
        };
        let patch = render(&paths, before, after, &lines);
        Ok(Built {
            patch,
            content: exists.then_some(content),
        })
    }
}

#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests;
