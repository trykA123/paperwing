use super::*;
use crate::commit::test_fixture::Fixture;

fn paths<'a>(old: &'a str, new: &'a str) -> Paths<'a> {
    Paths {
        old,
        new,
        before: true,
        after: true,
        mode: "100644",
    }
}

fn whole(hunk: usize) -> Selection {
    Selection { hunk, ranges: None }
}

fn final_line_selection(diff: &Diff<'_>, reverse: bool) -> Selection {
    let lines = &diff.hunks()[0].lines;
    let line = lines
        .iter()
        .position(|line| line.text == "c" && line.kind == if reverse { "remove" } else { "add" })
        .unwrap();
    Selection {
        hunk: 0,
        ranges: Some(vec![LineRange {
            start: line,
            end: line,
        }]),
    }
}

#[test]
fn missing_final_newline_refuses_append_without_selecting_its_last_line_change() {
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"a\nb");
    let diff = Diff::new(b"a\nb", b"a\nb\nc").unwrap();
    let result = diff.build(
        paths("file.txt", "file.txt"),
        &[final_line_selection(&diff, false)],
        false,
    );
    assert!(
        matches!(result, Err(error) if error == "Select the last line's change too: the file has no final newline")
    );
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nb");
    let complete = diff
        .build(paths("file.txt", "file.txt"), &[whole(0)], false)
        .unwrap();
    fixture.apply(&complete.patch, false);
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nb\nc");
}

#[test]
fn missing_final_newline_refuses_reverse_without_selecting_its_last_line_change() {
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"a\nb");
    let diff = Diff::new(b"a\nb\nc", b"a\nb").unwrap();
    let result = diff.build(
        paths("file.txt", "file.txt"),
        &[final_line_selection(&diff, true)],
        true,
    );
    assert!(
        matches!(result, Err(error) if error == "Select the last line's change too: the file has no final newline")
    );
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nb");
    let complete = diff
        .build(paths("file.txt", "file.txt"), &[whole(0)], true)
        .unwrap();
    fixture.apply(&complete.patch, true);
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nb\nc");
}
