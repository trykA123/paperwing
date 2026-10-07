use super::*;
use crate::commit::test_fixture::{two_hunks, Fixture};

fn whole(hunk: usize) -> Selection {
    Selection { hunk, ranges: None }
}
fn paths<'a>(old: &'a str, new: &'a str) -> Paths<'a> {
    Paths {
        old,
        new,
        before: true,
        after: true,
        mode: "100644",
    }
}

#[test]
fn stage_one_crlf_hunk_and_reverse_it_byte_for_byte() {
    let fixture = Fixture::new();
    let (before, after) = two_hunks();
    fixture.commit("with spaces.txt", &before);
    let diff = Diff::new(&before, &after).unwrap();
    assert_eq!(diff.hunks().len(), 2);
    let built = diff
        .build(
            paths("with spaces.txt", "with spaces.txt"),
            &[whole(0)],
            false,
        )
        .unwrap();
    fixture.apply(&built.patch, false);
    let indexed = fixture.git(&["show", ":with spaces.txt"]);
    assert_eq!(indexed, built.content.unwrap());
    assert!(fixture
        .git(&["diff", "--cached"])
        .windows(b"first change\r".len())
        .any(|line| line == b"first change\r"));
    assert!(!indexed
        .windows(b"second change".len())
        .any(|line| line == b"second change"));
    let reverse = Diff::new(&before, &indexed)
        .unwrap()
        .build(
            paths("with spaces.txt", "with spaces.txt"),
            &[whole(0)],
            true,
        )
        .unwrap();
    fixture.apply(&reverse.patch, true);
    assert_eq!(fixture.git(&["show", ":with spaces.txt"]), before);
}

#[test]
fn line_ranges_stage_and_unstage_only_selected_replacements() {
    let fixture = Fixture::new();
    let before = b"a\nb\nc\nd\n";
    let after = b"a\nB\nC\nd\n";
    fixture.commit("file.txt", before);
    let diff = Diff::new(before, after).unwrap();
    let positions: Vec<_> = diff.hunks()[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| matches!(line.text.as_str(), "b" | "B"))
        .map(|(index, _)| LineRange {
            start: index,
            end: index,
        })
        .collect();
    let selection = Selection {
        hunk: 0,
        ranges: Some(positions),
    };
    let built = diff
        .build(
            paths("file.txt", "file.txt"),
            std::slice::from_ref(&selection),
            false,
        )
        .unwrap();
    fixture.apply(&built.patch, false);
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nB\nc\nd\n");
    fixture.git(&["reset", "--hard", "-q", "HEAD"]);
    fixture.write("file.txt", after);
    fixture.git(&["add", "file.txt"]);
    let reverse = diff
        .build(paths("file.txt", "file.txt"), &[selection], true)
        .unwrap();
    fixture.apply(&reverse.patch, true);
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nb\nC\nd\n");
}

#[test]
fn no_final_newline_survives_replacement_in_both_directions() {
    let fixture = Fixture::new();
    for (before, after) in [
        (b"old\r\nlast".as_slice(), b"new\r\nLAST".as_slice()),
        (b"old\n".as_slice(), b"new".as_slice()),
        (b"old".as_slice(), b"new\n".as_slice()),
    ] {
        fixture.write("file.txt", before);
        fixture.git(&["add", "file.txt"]);
        let diff = Diff::new(before, after).unwrap();
        let forward = diff
            .build(paths("file.txt", "file.txt"), &[whole(0)], false)
            .unwrap();
        fixture.apply(&forward.patch, false);
        assert_eq!(fixture.git(&["show", ":file.txt"]), after);
        let reverse = diff
            .build(paths("file.txt", "file.txt"), &[whole(0)], true)
            .unwrap();
        fixture.apply(&reverse.patch, true);
        assert_eq!(fixture.git(&["show", ":file.txt"]), before);
    }
}

#[test]
fn renames_with_spaces_keep_other_changes_and_reverse_selected_changes() {
    let fixture = Fixture::new();
    let (before, after) = two_hunks();
    fixture.commit("old name.txt", &before);
    let diff = Diff::new(&before, &after).unwrap();
    let built = diff
        .build(paths("old name.txt", "new name.txt"), &[whole(0)], false)
        .unwrap();
    fixture.apply(&built.patch, false);
    assert_eq!(
        fixture.git(&["show", ":new name.txt"]),
        built.content.unwrap()
    );
    assert!(fixture
        .git(&["diff", "--cached", "--name-status", "-M"])
        .starts_with(b"R"));
    fixture.git(&["reset", "-q", "HEAD"]);
    fixture.git(&["mv", "old name.txt", "new name.txt"]);
    fixture.write("new name.txt", &after);
    fixture.git(&["add", "new name.txt"]);
    let reverse = diff
        .build(paths("old name.txt", "new name.txt"), &[whole(0)], true)
        .unwrap();
    fixture.apply(&reverse.patch, true);
    assert_eq!(
        fixture.git(&["show", ":new name.txt"]),
        reverse.content.unwrap()
    );
}

#[test]
fn new_and_deleted_files_support_partial_lines() {
    let fixture = Fixture::new();
    let after = b"first\r\nsecond";
    let diff = Diff::new(b"", after).unwrap();
    let selection = Selection {
        hunk: 0,
        ranges: Some(vec![LineRange { start: 0, end: 0 }]),
    };
    let built = diff
        .build(
            Paths {
                before: false,
                ..paths("new.txt", "new.txt")
            },
            std::slice::from_ref(&selection),
            false,
        )
        .unwrap();
    fixture.apply(&built.patch, false);
    assert_eq!(fixture.git(&["show", ":new.txt"]), b"first\r\n");
    fixture.write("new.txt", after);
    fixture.git(&["add", "new.txt"]);
    let deleted = Diff::new(after, b"")
        .unwrap()
        .build(
            Paths {
                after: false,
                ..paths("new.txt", "new.txt")
            },
            &[selection],
            false,
        )
        .unwrap();
    fixture.apply(&deleted.patch, false);
    assert_eq!(fixture.git(&["show", ":new.txt"]), b"second");
}

#[test]
fn binary_invalid_ranges_and_context_only_selection_are_refused() {
    assert!(Diff::new(b"\0", b"text").is_err());
    assert!(Diff::new(b"\xff", b"text").is_err());
    let diff = Diff::new(b"a\nb\n", b"a\nB\n").unwrap();
    for ranges in [
        vec![LineRange { start: 5, end: 9 }],
        vec![LineRange { start: 0, end: 0 }],
        vec![LineRange { start: 2, end: 1 }],
    ] {
        assert!(diff
            .build(
                paths("a", "a"),
                &[Selection {
                    hunk: 0,
                    ranges: Some(ranges)
                }],
                false
            )
            .is_err());
    }
}

#[test]
fn diff_reconstructs_every_small_sequence() {
    let texts: Vec<Vec<u8>> = (0..64)
        .map(|value| {
            (0..6)
                .map(|bit| {
                    if value & (1 << bit) == 0 {
                        "a\n"
                    } else {
                        "b\n"
                    }
                })
                .collect::<String>()
                .into_bytes()
        })
        .collect();
    for before in &texts {
        for after in &texts {
            let diff = Diff::new(before, after).unwrap();
            let old: Vec<u8> = diff
                .edits
                .iter()
                .filter(|edit| edit.kind != Kind::Add)
                .flat_map(|edit| edit.bytes.iter().copied())
                .collect();
            let new: Vec<u8> = diff
                .edits
                .iter()
                .filter(|edit| edit.kind != Kind::Remove)
                .flat_map(|edit| edit.bytes.iter().copied())
                .collect();
            assert_eq!(&old, before);
            assert_eq!(&new, after);
        }
    }
}
