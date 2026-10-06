# 29 — Discard changes and partial staging

Status: ready (Linux recovery records are merged; Windows path needs a check)
Platform: Windows first, Linux parity
Size: M
Role: both (backend first)

## Goal
The user reverts a file or a single change from the commit dialog, with an undo in Recovery. The user stages individual hunks and line ranges of a file before committing.

## Already done
- Commit dialog and Git commands: `src/components/CommitDialog.svelte`; `src-tauri/src/commit.rs` has `repo_changes`, `change_content`, `stage_paths`, `unstage_paths`, `commit_staged`. Staging works per file only.
- Recovery records and undo: `recovery_list`, `recovery_undo`, `recovery_cleanup`, `recovery_resolve`, in `src-tauri/src/files.rs` (Windows) and `src-tauri/src/linux_files/` (Linux), with `RecoveryPanel.svelte`.
- Missing: discard (file and hunk), hunk and line staging, the recovery record for a discard.

## Decisions
- Staging goes through `git apply --cached` with a patch Skein builds from a hunk or line selection. Fixed argv, patch on stdin, no shell.
- Every discard writes a recovery record first. If the record cannot be written, the discard does not run.
- The record stores the exact bytes of the file as they were, so undo restores them. Untracked file discard moves the file to recovery, never deletes it.
- Discard needs a confirmation naming every file. A hunk discard shows the hunk.
- Hunk and line actions refuse when the working tree changed since the diff was read (compare a content hash with the one returned by the diff command).

## Scope
- Do: discard file, discard hunk, stage and unstage hunk, stage and unstage line ranges, UI in the commit dialog and the file view.
- Do not: change the commit semantics; touch compare file copy or save paths.

## Read first
- `src-tauri/src/commit.rs`, `src/components/CommitDialog.svelte`, `src/components/DiffViewer.svelte`
- `src-tauri/src/files.rs` and `src-tauri/src/linux_files/` (recovery record API), `docs/linux-write-contract.md`
- `src/components/RecoveryPanel.svelte`, `src/lib/api.ts` (`RecoveryRecord`, `DiffArea`)

## Do not touch
- 26 owns drawer stash UI; 37 owns the compare editor.

## Steps
1. Patch builder (Rust, unit-tested): hunk and line selection to a valid patch, including CRLF files, no-newline-at-end, renames and binary refusal. Check: tests apply the patch to a scratch index and compare with `git diff --cached`.
2. Commands `stage_hunks`, `unstage_hunks` using `git apply --cached` (and `--reverse` for unstage), with the stale-content check. Check: Rust test where the file changes between read and apply.
3. Discard commands `discard_files` and `discard_hunk`: write the recovery record through the platform path (Windows `files.rs`, Linux `linux_files`), then restore from the index or remove the untracked file. Check: tests for the record, the result, and the "no record, no discard" rule; run the Windows tests on the VM.
4. UI: hunk and line checkboxes in the diff view, Stage and Unstage per hunk, Discard with confirmation. Check: browser test, both themes.
5. Undo from Recovery restores a discarded hunk or file. Check: fixture test, then a manual run on Windows.

## Done when
- Fixture: stage one of two hunks, commit, the other stays. Discard a hunk and undo it from Recovery.
- A file that changed after the diff was read is refused, not patched.
- Windows: discard and undo work on a path with spaces and CRLF content.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- A discard path cannot write its recovery record on Windows.
- The recovery record format needs a change.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
