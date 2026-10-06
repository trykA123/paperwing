# Packet 02 — comparison responsibility boundaries

**Status:** Proposed; requires approval. **Weight:** Standard. **Depends on:** accepted packet 01.
**Read first:** `src-tauri/src/compare.rs`, `paths.rs`, `files.rs` comparison callers, `clone.rs`, and the working plan's common verification rules.

## Outcome and scope
Extract comparison tests and coherent processing leaves without changing delivery, caches, IPC, filesystem protection or results. Keep `compare.rs` as the existing command/authority façade. New paths below are proposals.

## Requirements and invariants
- R1: Test, registration, inventory/content, text-diff and history responsibilities have named modules rather than continuing to accumulate in `compare.rs`.
- R2: All existing caller paths, serialized DTOs, final outputs and test discovery are unchanged.
- I1: Service/session/generation/fetch ownership, `Job` cancellation, safe roots and write-context authorization remain singular and fresh.
- I2: Normalized/binary/unavailable/count/history/rename/mode/link/submodule and budget semantics remain identical.

## Evidence
- [FACT] `compare.rs` has about 1,422 embedded test lines after its production code; stripping tests alone is not an engine redesign.
- [FACT] `registered_clone_destination`, `set_roots` and `registered_write_root` are consumed outside comparisons.
- [DECISION] Do not cache, optimize aggregation, change native paths, or introduce a new session architecture during these moves.

## Steps
- P1 [BATCH]: Move the unchanged child test module to proposed `src-tauri/src/compare/tests.rs`; retain private access, runner lock and packet-01 fixture resolution.
  - Depends on: none. Location: `compare.rs::mod tests`.
  - Verify: full serial Cargo tests and test-discovery comparison with 01.
  - Fails if: relocated fixtures/tests silently stop running.
- P2 [BATCH]: Move saved workspace/template binding to proposed `compare/registration.rs`; preserve façade exports for clone/trash and exact layout vectors.
  - Depends on: P1. Location: `text`, `js_space`, `template_segments`, `bind`, registered destination/root functions.
  - Verify: `bun test src/lib`; full serial Cargo tests, including shared layout vectors and registered destination protections.
  - Trap: fixing Windows/Linux sanitization or case identity while extracting it.
- P3 [ISOLATE]: Move inventory representations/read processing to proposed `compare/inventory.rs`. Pass the existing `Job` and safe root; keep authority-bearing values session-owned and use narrowly scoped visibility.
  - Depends on: P2. Location: `Kind`, `Entry`, `inventory`, `index_sizes`, `index_blobs`, `Resolved`, `content`, `fingerprint`.
  - Verify: full Cargo tests including `scaled_working_inventory_is_lazy_bounded_and_cancellable`; compare result fingerprints and invocation counts with 01.
  - Fails if: batching, cancellation, read limits, fingerprint checks or type classification changes.
- P4 [BATCH]: Move unchanged normalization/binary/count/numstat/temp handling to proposed `compare/text_diff.rs`; verify before moving history separately to `compare/history.rs`.
  - Depends on: P3. Location: `normalized`, `binary`, `Temporary`, `line_counts`, `count_result`, `diff_metadata`; then `HistorySource`, `history`.
  - Verify: full serial Cargo tests and final-option result fingerprints after each move.
  - Trap: replacing Git diff or changing temporary-file permission policy as an extraction.
- P5 [ISOLATE]: Audit the remaining coordinator and exports; record remaining mixed seams rather than splitting by line count. Confirm Tauri wrappers stay registered at original paths and no extracted module independently creates service/jobs/leases.
  - Depends on: P4. Location: `compare.rs::Service`, command wrappers; `lib.rs::generate_handler`, clone/files/trash callers.
  - Verify: `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; full Cargo tests; native comparison/copy authorization walkthrough against packet-01 fixtures.

## Acceptance
- A1 (static): dependency/export map shows cohesive leaves, unchanged IPC paths and one authority owner → R1/R2/I1.
- A2 (fixture): full tests, identical test discovery/result fingerprints/process counts → R2/I1/I2.
- A3 (native): comparison refresh/cancel/close and save/copy refusal on stale context match baseline → R2/I1.

## Verification commands
`bun run --bun check`; `bun test src/lib`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`; the clippy command above; `git -c core.whitespace=cr-at-eol diff --check`. Run after each move; no whole-packet unchecked rewrite.

## Stop and rollback
Stop on changed contracts, ownership, counts, results or missing native safety evidence. Restore only this packet's moves/imports; preserve tests/fixtures from 01 and unrelated edits. Intentional Linux/performance changes belong to later packets.

## Revision log
- 2026-10-02: Proposed behavior-preserving extraction.

- [REV 2026-10-03] P2 keeps registered_write_root in compare.rs because it constructs Job and validates read-root authority. Plain registration moves to the child, with two original-path forwarding wrappers. The1539-line remaining coordinator retains cohesive session/read-root/authorization ownership; further seams are recorded instead of splitting by size.
