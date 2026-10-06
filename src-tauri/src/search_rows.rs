use crate::search::{ContextLine, Match};

const MAX_LINE_CHARS: usize = 400;
const LEAD_CHARS: usize = 40;

pub enum Row {
    Match(Match),
    Context {
        path: String,
        line: u32,
        text: String,
    },
}

pub fn parse_row(row: &[u8], prefix: Option<&str>, context: bool) -> Option<Row> {
    let parts: Vec<&[u8]> = row.splitn(4, |byte| *byte == 0).collect();
    if parts.len() < 3 {
        return None;
    }
    let path = String::from_utf8_lossy(parts[0]).into_owned();
    let path = match prefix.and_then(|prefix| path.strip_prefix(prefix)) {
        Some(stripped) => stripped.to_string(),
        None => path,
    };
    let line: u32 = number(parts[1])?;
    let column = parts.get(3).and_then(|text| {
        let column: usize = number(parts[2])?;
        (!context || column.saturating_sub(1) <= text.len()).then_some(column)
    });
    if let (Some(column), Some(text)) = (column, parts.get(3)) {
        let (text, column) = window(text, column);
        return Some(Row::Match(Match {
            path,
            line,
            column,
            text,
            context: Vec::new(),
        }));
    }
    if !context {
        return None;
    }
    let mut raw = parts[2].to_vec();
    if let Some(rest) = parts.get(3) {
        raw.push(0);
        raw.extend_from_slice(rest);
    }
    Some(Row::Context {
        path,
        line,
        text: window(&raw, 1).0,
    })
}

fn number<T: std::str::FromStr>(bytes: &[u8]) -> Option<T> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn units(chars: &[char]) -> u32 {
    chars.iter().map(|c| c.len_utf16() as u32).sum()
}

pub fn window(raw: &[u8], column: usize) -> (String, u32) {
    let end = column.saturating_sub(1).min(raw.len());
    let offset = String::from_utf8_lossy(&raw[..end]).chars().count();
    let text = String::from_utf8_lossy(raw);
    let chars: Vec<char> = text.trim_end_matches('\r').chars().collect();
    let offset = offset.min(chars.len());
    if chars.len() <= MAX_LINE_CHARS {
        return (
            chars.iter().collect(),
            units(&chars[..offset.min(chars.len())]) + 1,
        );
    }
    let start = offset.saturating_sub(LEAD_CHARS);
    let stop = (start + MAX_LINE_CHARS).min(chars.len());
    let mut clipped = String::new();
    if start > 0 {
        clipped.push('…');
    }
    clipped.extend(&chars[start..stop]);
    if stop < chars.len() {
        clipped.push('…');
    }
    let lead = u32::from(start > 0);
    (clipped, lead + units(&chars[start..offset.min(stop)]) + 1)
}

pub fn build_matches(rows: Vec<Row>, context: u8) -> Vec<Match> {
    let mut found = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if let Row::Match(hit) = row {
            let mut hit = hit.clone();
            hit.context = around(&rows, index, context);
            found.push(hit);
        }
    }
    found
}

fn context_at(rows: &[Row], index: usize, path: &str, line: u32, span: u32) -> Option<ContextLine> {
    match rows.get(index)? {
        Row::Context {
            path: other,
            line: seen,
            text,
        } if other == path && seen.abs_diff(line) <= span => Some(ContextLine {
            line: *seen,
            text: text.clone(),
        }),
        _ => None,
    }
}

fn around(rows: &[Row], index: usize, context: u8) -> Vec<ContextLine> {
    let Row::Match(hit) = &rows[index] else {
        return Vec::new();
    };
    let span = u32::from(context);
    let mut before: Vec<ContextLine> = (0..index)
        .rev()
        .map_while(|at| context_at(rows, at, &hit.path, hit.line, span))
        .collect();
    before.reverse();
    let after =
        (index + 1..rows.len()).map_while(|at| context_at(rows, at, &hit.path, hit.line, span));
    before.into_iter().chain(after).collect()
}
