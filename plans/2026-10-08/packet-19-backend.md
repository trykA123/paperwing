# Packet 19 backend evidence

Worktree: `/mnt/Sabrent/homelab/.crew/skein-p19`; branch: `codex/p19`; baseline: `da2e5ce`.

The local Git comparison producer publishes immutable pending inventory before metadata, ordered classification and history. Fixed rows commit without touching the byte budget; budget rows commit in path order. Folders wait for their direct children. Final totals, rows, sizes, reasons, normalization, renames and history match the frozen legacy oracle across all four option combinations.

Producers own their tasks and readers. Cancellation, refresh, close, reload and exit end their generation and join cleanup. Producer registration happens under the session mutex, so close cannot miss a newly started producer. Pending rows never authorize edits or copies. Final rows still require fresh root, generation and regular-file checks; folder copy checks destination-only rows too.

Content uses four independent interactive permits. The Git runner has 32 admissions: enrichment is capped at 16, leaves the last slot free and yields to queued interactive work. Its admission precedes the filesystem read gate. The independent packet 17 pool remains 12 readers. The content window is 32 MiB per comparison and 64 MiB globally, with 8 MiB reserved from enrichment for interactive work. Classification reads one budget row at a time. Pull pages and publication batches are capped at 500 rows or about 1 MiB. Events preserve every state change and coalesce row updates at 100 ms intervals.

New Rust IPC types: `Hint`, `PendingRow`, `RowUpdate`, `State`, `Totals`, `Progress`; event payload: `Notice { id, generation, sequence, state }`. `CoreEvent::CompareProgress` maps to `compare-progress`. The two API wrappers have matching TypeScript types and Bun IPC tests.

## Commits and checks

### Step 1: `dbf58f5`

`CHANGELOG.md`, `src-tauri/src/commands/compare.rs`, `src-tauri/src/compare.rs`, `src-tauri/src/compare/producer.rs`, `src-tauri/src/compare/progressive.rs`, `src-tauri/src/compare/remote_service.rs`, `src-tauri/src/compare/tests.rs`, `src-tauri/src/compare/tests/progressive.rs`, `src-tauri/src/events.rs`, `src-tauri/src/kernel/events.rs`.

### Step 2: `d760e01`

`CHANGELOG.md`, `src-tauri/src/compare.rs`, `src-tauri/src/compare/access.rs`, `src-tauri/src/compare/classification.rs`, `src-tauri/src/compare/listing.rs`, `src-tauri/src/compare/ordered_commit.rs`, `src-tauri/src/compare/producer.rs`, `src-tauri/src/compare/progressive.rs`, `src-tauri/src/compare/publication.rs`, `src-tauri/src/compare/resolving.rs`, `src-tauri/src/compare/tests/progressive.rs`, `src-tauri/src/git.rs`, `src-tauri/src/git/admission.rs`, `src-tauri/src/git/runner.rs`.

### Step 3: `99065fb`

`src-tauri/src/compare.rs`, `src-tauri/src/compare/access.rs`, `src-tauri/src/compare/classification.rs`, `src-tauri/src/compare/flight.rs`, `src-tauri/src/compare/ordered_commit.rs`, `src-tauri/src/compare/producer.rs`, `src-tauri/src/compare/progress_events.rs`, `src-tauri/src/compare/progressive.rs`, `src-tauri/src/compare/publication.rs`, `src-tauri/src/compare/tests.rs`, `src-tauri/src/compare/tests/progressive.rs`, `src-tauri/src/compare/tests/progressive_lifecycle.rs`, `src-tauri/src/compare/tests/progressive_scale.rs`, `src-tauri/src/events/tests.rs`, `src-tauri/src/git/admission.rs`, `src-tauri/src/lib.rs`.

Step 1 checks: legacy serialization, generation, pending and completion state; full native suite at that step: 835 passed, seven ignored. Bun: 599 passed; check: zero errors and warnings.

Step 2 checks: 93 comparison tests and two runner admission tests; deferred fixture: inventory 15 ms, content 19 ms, complete 24 ms. The oracle option matrix passed.

Step 3 checks: lifecycle, stale generations, final-row authority, destination-only copy scope, panic supervision, consumer-less completion, memory admission and event coalescing. The 20,000-row deferred fixture produced 40,000 updates in 80 pages and released its runner/readers/content reservations.

### Step 4: commit containing this report

Files: `src-tauri/src/compare.rs`, `compare/{flight,listing,producer,progressive,publication,resolving,ref_resolution}.rs`, `compare/tests/{equivalence,progressive_lifecycle,progressive_scale}.rs`, `src-tauri/src/git/runner.rs`, `src/lib/api.ts`, `src/lib/api-progressive.test.js`, this report and `docs/progressive-command-inventory.json`.

The supervised producer now participates in every oracle option matrix. Added budget pre-check/post-check, gitlink, opaque nested repository and Unicode/space-path cases. The nondeferred scale trace proves early inventory and selected content while enrichment is still running. The start/close regression fails with the previous registration code (`close returned before producer ownership was registered`) and passes with atomic registration. Both legacy refresh and progressive start now send progress events through the same producer.

## Final gates

All commands ran in this worktree; native test temporary storage was inside it.

```sh
cd /mnt/Sabrent/homelab/.crew/skein-p19/src-tauri
SKEIN_TEST_TMP=/mnt/Sabrent/homelab/.crew/skein-p19/.p19-test-tmp TMPDIR=/mnt/Sabrent/homelab/.crew/skein-p19/.p19-test-tmp cargo test --offline
cargo clippy --all-targets --offline
rustfmt --edition 2021 --config skip_children=true --check <the 15 new Rust files>
cd /mnt/Sabrent/homelab/.crew/skein-p19
bun test /mnt/Sabrent/homelab/.crew/skein-p19/src/lib
bun run --bun check
```

Native: 851 passed, zero failures, seven existing ignores; 156.63 s. Clippy: successful, no new warning messages; existing baseline remains 19 library warnings and 11 test warnings, including six duplicates. Format: all 15 new Rust files passed. Bun: 602 passed, zero failures. Svelte/TypeScript check: 1,043 files, zero errors and warnings. `git diff --check` passed with CRLF-aware whitespace handling.

Additional checks: the nondeferred native scale trace passed; `cargo test --offline --features test-profile --lib progressive` passed 14 tests, including benchmark-enabled compilation and the ownership regression.

## Native timing

The final nondeferred 20,000-row run produced inventory at **328 ms**, selected content at **365 ms**, and completion at **4297 ms**. The content test asserted that the session was still `enriching` after the read. First inventory availability was 7.6% of full completion.

The producer retained 40,000 updates, exactly one pending and one final update per row, delivered in 80 bounded pages. Final fingerprint: `7aabd3ab506a60f70bdfcf01b3b6cb50a6d0419fcdd1a4fceb044452f9894a5e`.

```text
cache=app-empty os-cache=uncontrolled prewarm=off rows=20000 listed=328ms content=365ms complete=4297ms pages=80
``` Each run uses a new repository and a fresh service, with app comparison caches empty and prewarm off. The operating-system page cache is uncontrolled. These times measure backend availability; frontend rendering belongs to packet 20.

## Command inventory and preservation

[Recorded inventory](../../docs/progressive-command-inventory.json) compares domain registrations at `da2e5ce` against this implementation, including conditional commands: 100 before, 102 after. Default Linux/Windows builds register 92 before and 94 after. Added only `comparison_start` and `comparison_progress`; removed none.

No new crates. Cargo manifests and lockfile are unchanged. `git/locale.rs`, `git/binary.rs`, `git/batch.rs`, `files.rs` and `file_guard.rs` are byte-identical to baseline. Runner command environment, binary/locale routing, Windows creation flags and stdin stream/capture handling are byte-identical. Existing LF/CRLF conventions are preserved. No UI, caching or prewarm was added. GitHub comparison behavior remains compatible with its existing final-result path; the early-publication trace exercises local Git.

## Windows proof still required

No local Windows run and no CI run occurred. No push was authorized. The existing `test-windows` job in `.github/workflows/build.yml` runs on a code push, including parallel native tests and `cargo test --locked`.

Cross-platform tests exercise Unicode/space paths, all option combinations, budget boundaries, the 20,000-row pipeline, cancellation, close/reload/refresh, producer panic, final-only write/copy authority, admission priority and content-memory limits. Their Windows execution remains pending CI.

Only Windows execution can validate the native process termination and handle-release paths, actual Git for Windows binary/PATH/locale behavior, NTFS reparse/junction swaps and extended paths, and real Windows file-lock contention during cancel/refresh/close. The owner's PC must supply diagnostics for the largest repository, selected-file opening and a ref switch during enrichment, actual first rendered row timing, and the under-two-second first-response target. Those hardware/UI measurements cannot be proved by Linux backend fixtures.

## Cleanup

Removed both session-owned `.p19-test-tmp` trees after evidence capture: the worktree root and the quota-test evidence subtree. The pre-existing shared `node_modules` symlink remains untouched.
