use super::{Edit, Kind, Paths};

fn quote(path: &str) -> String {
    let mut result = String::from("\"");
    for byte in path.bytes() {
        match byte {
            b'"' => result.push_str("\\\""),
            b'\\' => result.push_str("\\\\"),
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            32..=126 => result.push(byte as char),
            _ => result.push_str(&format!("\\{byte:03o}")),
        }
    }
    result.push('"');
    result
}

pub(super) fn render(paths: &Paths<'_>, before: bool, after: bool, lines: &[Edit<'_>]) -> Vec<u8> {
    let old = quote(&format!("a/{}", paths.old));
    let new = quote(&format!("b/{}", paths.new));
    let mut patch = format!("diff --git {old} {new}\n");
    if !before {
        patch.push_str(&format!("new file mode {}\n", paths.mode));
    }
    if !after {
        patch.push_str(&format!("deleted file mode {}\n", paths.mode));
    }
    if before && after && paths.old != paths.new {
        patch.push_str(&format!(
            "rename from {}\nrename to {}\n",
            quote(paths.old),
            quote(paths.new)
        ));
    }
    patch.push_str(&format!(
        "--- {}\n+++ {}\n",
        if before { &old } else { "/dev/null" },
        if after { &new } else { "/dev/null" }
    ));
    let mut patch = patch.into_bytes();
    let (mut cursor, mut old_offset, mut new_offset) = (0, 0, 0);
    for span in super::spans(lines) {
        let (old_skip, new_skip) = counts(&lines[cursor..span.start]);
        old_offset += old_skip;
        new_offset += new_skip;
        let (old_count, new_count) = counts(&lines[span.clone()]);
        patch.extend_from_slice(
            format!(
                "@@ -{},{old_count} +{},{new_count} @@\n",
                old_offset + usize::from(old_count != 0),
                new_offset + usize::from(new_count != 0),
            )
            .as_bytes(),
        );
        render_lines(&mut patch, &lines[span.clone()]);
        old_offset += old_count;
        new_offset += new_count;
        cursor = span.end;
    }
    patch
}

fn counts(lines: &[Edit<'_>]) -> (usize, usize) {
    (
        lines.iter().filter(|line| line.kind != Kind::Add).count(),
        lines
            .iter()
            .filter(|line| line.kind != Kind::Remove)
            .count(),
    )
}

fn render_lines(patch: &mut Vec<u8>, lines: &[Edit<'_>]) {
    for line in lines {
        patch.push(match line.kind {
            Kind::Context => b' ',
            Kind::Add => b'+',
            Kind::Remove => b'-',
        });
        patch.extend_from_slice(line.bytes);
        if !line.bytes.ends_with(b"\n") {
            patch.extend_from_slice(b"\n\\ No newline at end of file\n");
        }
    }
}
