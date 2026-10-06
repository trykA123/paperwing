# Packet 10 — Linux secure credential persistence

**Status:** Implemented on Linux; entropy/runtime repairs independently approved and combined native verification passes. Windows/owner-observed acceptance pending. **Weight:** Heavy (auth). **Depends on:** accepted 04,09.
**Read first:** `Cargo.toml`/`Cargo.lock`, `settings.rs` token functions, `github.rs` authentication, Settings source form, API and platform capabilities.

## Outcome and scope
GitHub tokens persist securely across Linux restarts, with honest locked/unavailable/backend errors and unchanged Windows behavior. No plaintext fallback, homemade encryption, token caching, broad credential migration or new login system.

## Requirements and invariants
- R1: A reviewed persistent Linux OS credential backend is enabled for the pinned keyring version and reported in capabilities.
- R2: Save/read/delete/restart, locked store and unavailable service have distinct actionable outcomes without losing tokens.
- R3: Source/token edits invalidate scoped data through a nonsecret revision/invalidation seam for packet 16.
- I1: Tokens never reach settings/cache keys/logs/fixtures; normal Windows `paperwing`/legacy `flock` entries are preserved.
- I2: Test profile uses isolated credential service/source IDs; no overwrite/delete of unrelated credentials.

## Evidence
- [FACT] Manifest enables only `windows-native`/`apple-native`; `get_token` currently collapses errors into absence, and Settings promises Windows Credential Manager.
- [ASSUMPTION] A persistent desktop credential service is available on the reference Linux session; P1/P3 determine this, not the build succeeding.
- [DECISION] Prefer a supported persistent desktop secret-store backend; exact feature/API/dependencies require pinned-source verification, not a guessed feature name. Ephemeral session key storage does not prove persistence.

## Steps
- P1 [ISOLATE]: Inspect pinned keyring source/docs and reference desktop's available persistent store; document feature/backend, dependencies, locked/unavailable behavior and namespace isolation. Submit auth/dependency decision for approval.
  - Depends on: none. Location: Cargo manifest/lock and proposed credential design note/evidence.
  - Verify: reviewed exact version/API/feature names, native availability probe without displaying credentials; do not install packages or refresh user auth implicitly.
- P2 [ISOLATE]: Add target-specific Linux backend and explicit credential status/errors; retain current command contracts through compatibility wrappers or an approved additive DTO. Wire platform capability and source-form messages to actual backend, not OS-name assumptions.
  - Depends on: P1 approved choice. Location: `settings.rs::{entry,get_token,set_token,has_token,delete_token}`, `Cargo.toml`, platform/API/Settings.
  - Verify: full Cargo tests/clippy, frontend check/tests/build and exact legacy payload fixtures on Windows/Linux.
- P3 [ISOLATE]: Exercise isolated source token create/read/restart/delete, locked/unavailable/reconnected store, and API permission denial. Add nonsecret revision notification/invalidation seam without persisting token hashes.
  - Depends on: P2. Location: native credential fixture and GitHub request/cache readers.
  - Verify: owner-observed native Linux and Windows restarts plus error-path tests; inspect settings/cache/report files for secret absence.
  - Trap: `has_token == false` masking a locked store and then overwriting the existing credential.

## Acceptance
- A1 (static + native): exact persistent backend choice/feature and native availability/status → R1/I1.
- A2 (native owner-observed): restart/delete/locked/unavailable recovery matrix → R2/I1/I2.
- A3 (fixture): nonsecret scoped invalidation seam and unchanged Windows/legacy/test credential namespaces → R3/I1/I2.

## Heavy authority, stop and rollback
User approves backend/dependencies and sacrificial credential drills. One writer in a dedicated worktree; main session integrates. Back up only test source configuration and prove deleting/recreating the sacrificial token; never export real credentials into evidence. Rollback restores owned backend/config/UI changes and deletes only approved test credential IDs. Existing tokens remain in their OS store. Stop on plaintext persistence, ambiguous store errors, unexpected auth data, unavailable persistence or unaccepted migration.

## Revision log
- 2026-10-02: Proposed native credential gate; no keyring API/backend choice claimed verified.

- [REV 2026-10-03] P1: pinned keyring3.6.3 source and read-only native bus probes show a mock Linux backend and unavailable Secret Service. Proposed durable desktop Secret Service/RustCrypto plus bounded no-prompt Linux adapter; exact new dependency source checksum verified without execution. See docs/linux-credentials.md and ignored10 evidence. Product/lockfile changes and sacrificial credential drills remain gated by the named decision.

- [REV 2026-10-03] Owner explicitly approved continued work and the concrete backend/dependency/isolated-wallet proposal. No repeated authorization request is needed. Review verified prompt/RPC semantics and identified committed-but-timed-out ambiguity; revision advances before dispatch and uncertain outcomes retain user input/source. Final independent review is pending a model usage limit; implementation acceptance is not claimed.

- [REV 2026-10-03] P2/P3: implemented the approved backend, typed status, pre-dispatch revision and uncertain mutation handling. Fresh private native KSecrets n4 proves restart/locked/unavailable/denied/delete/recreate and isolated authenticated HTTP 401/403. Bun61, Rust47/52 and Svelte checks pass. Browser390/1440 light/dark verifies failed save/delete/cancel-new and Manual no-store workflows. Fixed reproduced Tooltip cleanup and nondeterministic timeout fixture. Strict Clippy retains existing Linux dead-code failures. Bounded27-file delta integrated on feat/paperwing-linux-completion; no commit, push, real-wallet change or full parity claim. Final independent review remains pending a model usage limit.

- [REV 2026-10-04] Independent review approved fallible OS entropy for DH/CBC in the pinned vendored adapter and26-file runtime repair: pre-HTTP revision freshness, quiet local Git during inaccessible credentials, managed Manual ownership and captured display redaction. Runtime65/70 tests and16 browser cases pass; six actual fail-before regressions are recorded. Integrated into main; combined-source suites and private native recheck pending.

- [REV 2026-10-04] Combined80/85 Rust tests,63 frontend tests, diagnostic Clippy and normal/test-profile builds pass. Fresh private KSecrets n5 repeats all eight native acceptance checks with OS entropy/runtime fixes; two native comparisons preserve baseline results/counters. The normal binary excludes test hooks. Native Windows, real desktop wallet configuration and owner-observed matrix remain pending.
