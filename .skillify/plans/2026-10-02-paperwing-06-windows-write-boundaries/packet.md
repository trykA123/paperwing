# Packet 06 — extract Windows write responsibilities without weakening them

**Status:** Proposed; requires approval. **Weight:** Heavy. **Depends on:** accepted 01; native Windows test environment mandatory.
**Read first:** `src-tauri/src/files.rs`, `file_guard.rs`, `clone.rs::run_job`, `lib.rs` and README recovery/safety sections.

## Outcome and scope
Separate the existing Windows journal, tickets and frozen copy plans from IPC wrappers. No Linux implementation, shared transaction trait, record/schema change, fallback writer or safety cleanup here. Proposed destinations are not existing code.

## Requirements and invariants
- R1: `files.rs` becomes a façade for cohesive `files/{journal,tickets,copy}.rs`; Windows primitive code may move unchanged to `file_guard/windows.rs`.
- R2: Existing Windows IPC/records and all fault/alias/lock/undo tests behave identically.
- I1: Preserve TxF scope/drop order, pinned ancestors, expected bytes, identity, security/attributes, stream restrictions, backup order and journal locking.
- I2: One ticket/plan service and global filesystem gate retain ownership; existing records remain readable/restorable.

## Evidence
- [FACT] `files.rs` is Windows-only, contains journal tests in the middle, and ends with production ticket/copy/editor/recovery commands.
- [FACT] `files::identity` is used by reclone; command paths are directly registered in `lib.rs`.
- [DECISION] Mechanical moves must not deduplicate similar validation or broaden Rust visibility to blanket `pub`.

## Steps
- P1 [ISOLATE]: Run/list all Windows file-guard/journal tests; restore a sacrificial record with the baseline executable. Record ignored probes and required TxF/NTFS capability.
  - Depends on: none. Location: `files.rs::tests`, `file_guard.rs::tests`, packet-01 isolated profile.
  - Verify: `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`; native save/restart/undo drill on disposable NTFS.
  - Fails if: no native Windows evidence exists; Linux compilation cannot substitute.
- P2 [ISOLATE]: Move `Record`, `Journal`, `WritePolicy`, durable/backup/security helpers and their tests to `files/journal.rs`, preserving private test access and façade exports.
  - Depends on: P1. Location: journal portion of `files.rs`.
  - Verify: same Windows tests/result/record-byte fixtures; `clone.rs` still builds against `files::identity`.
- P3 [ISOLATE]: Move ticket ownership and frozen copy orchestration to `files/tickets.rs` and `files/copy.rs`; keep Tauri wrappers/service registration at original façade paths.
  - Depends on: P2. Location: `Ticket`, `Service`, `lock_context`, `FrozenCopy`, `CopyPlan`, preview/apply/cancel and edit/save command internals.
  - Verify: native stale ticket/preview, copy partial/cancel, dirty editor close and page-reload ticket release.
  - Trap: creating a separate gate or cloning authority-bearing tickets into a globally reusable cache.
- P4 [ISOLATE]: If needed, move native guard implementation unchanged behind a Windows-only façade; confirm serialization/macros/callers and inspect full diff.
  - Depends on: P3. Location: `file_guard.rs`, `lib.rs`, clone/files imports.
  - Verify: full Windows tests/clippy and native recovery matrix; Linux cargo check still excludes Windows code rather than enabling ordinary writes.

## Acceptance
- A1 (static + native fixtures): ownership/import map and unchanged serialized records/IPC → R1/R2/I2.
- A2 (native fixtures): atomic replacement, locks/aliases, all fault checkpoints, torn/full journal, changed root/streams/hardlinks/modified-created-file undo → R2/I1/I2.
- A3 (owner-observed): baseline-created record survives extraction, restart and safe undo → R2/I2.

## Heavy authority and rollback
User approves this extraction, not a Linux protection model. Dedicated worktree/branch, one writer; main session integrates. Protect existing settings/recovery and prove sacrificial restore before moves. Restore only owned source/import changes on semantic drift; no persistent format changes means no record migration or cleanup is permitted. Stop on failed recovery drill, altered TxF/lock behavior, missing native proof or unexplained data drift.

## Commands
Full serial Cargo tests and `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` on Windows; frontend common gates; `git -c core.whitespace=cr-at-eol diff --check`.

## Revision log
- 2026-10-02: Proposed safety-sensitive extraction, native Windows gate retained.
