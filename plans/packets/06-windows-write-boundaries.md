# 06 — Windows write boundaries

Status: ready (Windows VM and the CI `test-windows` job both exist now)
Platform: Windows only. Linux must still compile with the Windows code excluded.
Size: M
Role: api-builder (gpt-6.1-sol xhigh), one writer

## Goal
Split `src-tauri/src/files.rs` (887 lines) into cohesive modules without changing any behaviour. Journal, tickets and frozen copy plans stop living in one file with the IPC commands. This is a mechanical move: no new features, no cleanup.

## Already done
- `src-tauri/src/files.rs` holds everything: `Record`, `Journal` and `WritePolicy` (lines 20–370), journal tests (370–578), `Ticket`, `Service`, `FrozenCopy`, `CopyPlan`, `lock_context` (578–635), copy commands `copy_preview`, `copy_cancel`, `copy_apply`, edit commands `file_edit_open`, `file_edit_close`, `file_save`, and recovery commands `recovery_list`, `recovery_undo`, `recovery_cleanup`, `recovery_resolve`.
- `src-tauri/src/file_guard.rs` (332 lines) holds `PinnedPath`, `Transaction` (TxF), `atomic_replace`, `pin_metadata`.
- `files::identity` is used by `clone.rs` (reclone). Registration in `src-tauri/src/lib.rs` is per platform (`#[cfg(windows)] files::…`, `#[cfg(target_os = "linux")] linux_files::…`).
- Linux has its own accepted service in `src-tauri/src/linux_files/`. Do not merge the two.
- CI runs `cargo test --locked` on `windows-latest` (`.github/workflows/build.yml`, job `test-windows`). The Windows VM is described in `plans/2026-10-06/handoff.md`.

## Decisions
- `files.rs` stays the façade. New modules: `files/journal.rs`, `files/tickets.rs`, `files/copy.rs`. Native primitives may move unchanged to `file_guard/windows.rs`.
- Moves are mechanical. Do not deduplicate similar validation. Do not widen visibility to `pub`; use `pub(super)` or `pub(crate)`.
- Preserve TxF scope and drop order, pinned ancestors, expected bytes, identity checks, security and attribute copying, stream restrictions, backup order and journal locking.
- One ticket service and one global filesystem gate keep ownership. Existing journal records stay readable and restorable. Serialized records and IPC shapes do not change.
- No shared transaction trait with Linux. The two backends keep their own journals.

## Scope
- Do: the extraction above, with tests moving alongside the code.
- Do not: Linux code, record or schema changes, a fallback writer, safety cleanup, behaviour changes.

## Read first
- `src-tauri/src/files.rs`, `src-tauri/src/file_guard.rs`
- `src-tauri/src/clone.rs` (the `files::identity` and `PinnedPath` callers), `src-tauri/src/lib.rs`
- `README.md` sections "Recovery and limits" and "Where things are stored"
- `~/.agents/rules/rust.md`, `~/.agents/rules/code-quality.md`

## Do not touch
- `src-tauri/src/linux_*` and `src-tauri/src/linux_files/`.
- Any packet 17 files (`compare/`, `git/runner.rs`).

## Steps
1. Baseline on Windows. List and run all file-guard and journal tests, then restore a sacrificial record using the unmodified build. Note which tests are ignored and what NTFS capability they need. Check: `cargo test --locked --lib -- --test-threads=1` on the Windows VM passes; a save, restart and undo drill on a disposable NTFS folder works.
2. Move `Record`, `Journal`, `WritePolicy` and the durable, backup and security helpers, with their tests, to `files/journal.rs`. Keep façade re-exports. Check: same Windows test results; `clone.rs` still builds against `files::identity`; a record created before the move still restores.
3. Move `Ticket`, `Service`, `lock_context`, `FrozenCopy` and `CopyPlan` to `files/tickets.rs` and `files/copy.rs`. Keep the Tauri commands registered at their original paths. Check: stale ticket and stale preview refused; copy partial and cancel; dirty editor close; ticket release on page reload.
4. Only if it helps, move native guard code unchanged to `file_guard/windows.rs`. Check: full Windows tests and clippy; `cargo check` on Linux still excludes the Windows code.
5. Review the full diff for accidental behaviour changes. Check: serialized record bytes and the command list are identical before and after.

## Done when
- Windows tests pass: atomic replace, locks, aliases, every fault checkpoint, torn and full journal, changed root, streams, hardlinks, undo of a modified or created file.
- A record written by the old build restores after the move, after a restart.
- Linux compiles and its tests are unchanged.

## Gates
- `cd src-tauri && cargo test --locked -- --test-threads=1` on the Windows VM and in CI
- `cargo clippy --all-targets -- -D warnings` on Windows
- `rustfmt --check` on touched files
- `bun run --bun check`, `bun test src/lib`

## Stop and report if
- The Windows VM or CI is unavailable (no Linux substitute counts as proof).
- A move needs a behaviour change, changes TxF or lock order, or the recovery drill fails.
- Records or IPC shapes change.

## Report
Commit sha, the moved-symbol map, the baseline and final Windows test counts, the recovery drill result, gate results.
