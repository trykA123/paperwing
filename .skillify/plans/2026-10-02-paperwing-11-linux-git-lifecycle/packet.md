# Packet 11 — Linux Git process lifecycle and cancellation

**Status:** Owner approved continued plan work; native ownership design and fixtures underway. **Weight:** Standard. **Depends on:** accepted 07,09.
**Read first:** extracted Git runner, `clone.rs::run`, commit commands and packet-01 fixture/Activity evidence.

## Outcome and scope
Linux Git operations have bounded output/time and cancellation that reaps owned helper descendants/pipes. Preserve the Windows runner and Git command semantics. No scheduling/prewarm, shell interpolation, speculative Git mutation or claim that cancellation rolls back a completed Git action.

## Requirements and invariants
- R1: Timeout/cancel handles direct Git plus owned helpers without orphaned output-drain hangs or leaked runner slots.
- R2: Spawn failure, late cancellation, helper exit and repeated cancellation have accurate Activity outcomes and redacted bounded output.
- I1: Process isolation/termination targets only this job; never kill unrelated Git/SSH processes.
- I2: Existing concurrency, command args, full-index commit/separate push and partial mutation semantics are unchanged.

## Evidence
- [FACT] Windows termination uses `taskkill /T`; non-Windows source kills/reaps the direct child. Descendant behavior is unproven.
- [DECISION] Verify Linux process-group/session mechanism against current Tokio/native APIs before adopting it; do not use broad process-name termination.

## Steps
- P1 [BATCH]: Add deterministic local helper fixtures with descendants holding stdout/stderr open, delayed exit, spawn failure and cancellation races.
  - Depends on: none. Location: Git runner tests and proposed `scripts/testing/process-fixtures/`.
  - Verify: demonstrate a positive control hanging/leaking under the unprotected fixture and record owned PIDs/pipe outcomes without external GitHub access.
- P2 [ISOLATE]: Implement bounded Linux job ownership/termination using a reviewed native mechanism; drain/close/reap all owned resources and preserve Windows branch behavior.
  - Depends on: P1. Location: runner `git` construction, `execute_inner`, `terminate`, timeout/cancel/cleanup paths.
  - Verify: full serial Cargo tests, clippy and repeated descendant/timeout/cancel fixtures; confirm unrelated control process survives.
  - Trap: marking Activity canceled while helpers or output drains still hold the semaphore.
- P3 [ISOLATE]: Exercise native clone/fetch/local comparison and a canceled helper on both platforms; report a Git mutation that completed before cancellation accurately.
  - Depends on: P2. Location: packet-01 marked local repositories and Activity drawer.
  - Verify: frontend common gates and owner-observed process/Activity state after cancel, including no leaked job slots.

## Acceptance
- A1 (native fixtures): descendant/pipe cleanup, bounded timeout and unrelated-process survival → R1/I1.
- A2 (fixture + owner-observed): spawn/late-cancel/repeated-cancel outcomes and unchanged command semantics → R2/I2.

## Stop and rollback
Stop on unreviewed syscall assumptions, orphan processes, broadened kill scope, secret leaks or semantic drift. Restore only owned runner changes and clean only recorded fixture PIDs/roots. Native process evidence is required; mocked `Child.kill` is not proof.

## Revision log
- 2026-10-02: Proposed Linux lifecycle behavior packet following mechanical runner extraction.

- [REV 2026-10-03] Owner approved the remaining plan and useful improvements. Read-only reconnaissance locates early job-registration removal and direct-child-only Linux termination. Primary-source research establishes identity-safe pidfd process-group signaling on Linux6.9+; retained-child fallback for older kernels is being checked before implementation. Group-escaping helpers require an explicit support limitation; no arbitrary descendant capture is claimed.

- [REV 2026-10-03] Implemented Linux owned pidfd/process-group jobs, retained registration and bounded stream cleanup. Both full serial profiles pass (56/61), three fresh Git repetitions pass and 27 recorded groups are gone. Two instrumented native release samples match baseline result and every command count. Normal ELF excludes test hooks. Independent Sol6.1/xhigh review is pending quota reset; native Windows and owner-observed acceptance remain unproven. Evidence: `.skillify/evidence/paperwing/11/frozen-delta.json`.

- [REV 2026-10-04] Independent review requested three lifecycle repairs. Native deterministic regressions fail with old decisions and pass after normal-exit preservation, Activity retention and awaited failed-capture reaping. Full59/64 suites and diagnostic Clippy pass;24 reports72 identities gone. Sol6.1/xhigh recheck approves the frozen repair. Four owned files integrated, bounded legacy aliases retained; Windows bodies unchanged. Combined native comparison still awaits credential repair build.
