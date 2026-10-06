# Packet 07 — Git runner and query responsibility boundaries

**Status:** Proposed; requires approval. **Weight:** Standard. **Depends on:** accepted 01.
**Read first:** `src-tauri/src/git.rs`, its clone/commit/local/settings/paths callers, and working plan verification rules.

## Outcome and scope
Extract Git responsibilities mechanically, retaining command/compatibility façades. No new scheduling, path rules, process-group cancellation, redaction policy, timeout or concurrency limits in this packet.

## Requirements and invariants
- R1: Runner/activity/cancellation ownership, redaction, validation, tree queries, remote refs/sorting and tests have named smaller modules.
- R2: Existing command paths, public Rust call sites and test discovery/results remain compatible.
- I1: Keep one semaphore, activity store, cancellation registry, filesystem gate and test runner lock; preserve output limits/redaction/draining/timeout behavior.

## Evidence
- [FACT] `git.rs` embeds tests in the middle and has production remote-ref code after them.
- [FACT] `execute_inner` uses a global 32-slot semaphore; moving the runner must not duplicate it.
- [DECISION] Linux descendant lifecycle is a later behavior packet, not a move-time fix.

## Steps
- P1 [BATCH]: Move unchanged tests to proposed `git/tests.rs`; preserve test hooks and single `TEST_RUNNER_LOCK` access.
  - Depends on: none. Location: `git.rs::mod tests`.
  - Verify: serial Cargo test discovery/results before and after.
- P2 [BATCH]: Move `redact`/secret-safe output helpers to `git/redaction.rs`; validation to `git/validation.rs`, maintaining façade exports and caller paths.
  - Depends on: P1. Location: redaction and `valid_url`/`valid_ref`/`valid_path`/`valid_root` functions.
  - Verify: redaction split-record/secret fixtures and existing path/ref validation fixtures; clippy.
  - Trap: introducing platform-specific filename rules during extraction.
- P3 [BATCH]: Move repository tree DTOs/query to `git/repository_tree.rs` and remote refs/natural ordering to `git/remote_refs.rs` one at a time.
  - Depends on: P2. Location: `repository_tree`, `natural_cmp`, `ls_remote`, `get_refs_many`; Tauri wrappers remain in façade.
  - Verify: tree and remote-ref output fixtures, natural sort characterization, exact command/payload compatibility.
- P4 [ISOLATE]: Move runner, Activity, cancellation and shared global owners together to `git/runner.rs`; leave forwarding exports/wrappers in `git.rs`.
  - Depends on: P3. Location: `Request`, `Captured`, drain/execute/terminate and Activity functions/static state.
  - Verify: full serial tests including late cancellation/drain/output bound cases; native Activity clear/cancel and clone/commit smoke against marked fixtures.
  - Fails if: activity/cancellation/gate state becomes per-import/per-worker instead of singleton.

## Acceptance
- A1 (static + fixture): cohesive ownership and unchanged public paths/test discovery → R1/R2/I1.
- A2 (fixture + native): redaction, bounds, drain/cancel, repository-tree/ref sorting and callers match baseline → R2/I1.

## Commands
`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`; `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; `bun run --bun check`; `bun test src/lib`; `git -c core.whitespace=cr-at-eol diff --check` after each move.

## Stop and rollback
Stop on changed process/query behavior, singleton duplication, exposed secrets or lost tests. Restore only this packet's owned moves/exports. New Linux/native semantics belong in 09/11.

## Revision log
- 2026-10-02: Proposed behavior-preserving Git decomposition.
- 2026-10-03: Added stable natural-sort characterization before extraction (28 default tests). Five transparent compatibility aliases preserve otherwise unused public Rust DTO paths. Private moved fields use `pub(super)` to retain the original `git` access scope. Strict dead-code diagnostics remain explicit; Linux gates and independent review passed, Windows execution unavailable.
