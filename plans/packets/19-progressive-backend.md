# 19 — Progressive comparison backend

Status: blocked by 18 (accepted contract)
Platform: Windows first, Linux parity
Size: L
Role: api-builder (gpt-6.1-sol xhigh), one writer

## Goal
The backend publishes the list of changed files as soon as it knows it, then fills in classification, counts and history. Old clients get today's final result. The UI switches in packet 20.

## Already done
- Nothing progressive exists. `Service::refresh` in `src-tauri/src/compare.rs` waits for the whole `prepare` before returning. `Job`, `Prepared`, `FILE_LIMIT` (20,000) and `BYTE_LIMIT` (64 MiB) are in the same file.
- The Git runner (`src-tauri/src/git/runner.rs`) has one semaphore of 32 slots and no priority classes. `git/batch.rs` (packet 17) reserves 30 slots for readers.
- No budget module exists (`src-tauri/src/budget.rs` is absent). Packet 21 defines it; ratify it before this packet starts.

## Decisions
- A new additive producer mode with generation-tagged batches. Names and types come from packet 18. Legacy commands return identical final output; progressive output converges to the same fingerprint.
- Two admission classes in the runner: `interactive` (anything the user asked for directly: open a file, pick a ref, refresh, start a comparison) and `enrichment` (classification, counts, history). Interactive work never waits behind queued enrichment. Enrichment uses at most half the slots, never takes the last free slot, and yields between bounded batches. Packet 22 later adds a lower `speculative` class to the same owner.
- Limits from the app budget: at most 32 MiB in flight per comparison and 64 MiB across all; batches of at most 500 rows or 1 MiB; progress events at most ten per second.
- Session generation, root, ref and metadata checks stay fresh. Provisional rows and retained producer data never grant write authority.
- No global session or handle cache, no per-file event flood, no early "final" summary, no change to normalisation or budget semantics.

## Scope
- Do: data types, commands and state machine; split publication (inventory, then selected-file content, then enrichment); admission classes; backpressure, completion and cancellation.
- Do not: UI changes, caching, prewarm.

## Read first
- `docs/progressive-comparison-contract.md` (from 18)
- `src-tauri/src/compare.rs`, `src-tauri/src/compare/*.rs`, `src-tauri/src/git/runner.rs`
- `src-tauri/src/lib.rs` (command registration)
- `src-tauri/src/compare/tests/legacy/` and `docs/testing.md`

## Do not touch
- `src/` frontend (packet 20), `files.rs`, `file_guard.rs`, `linux_*`.

## Steps
1. Add the types, commands and state machine from 18 with stable row ids. Legacy payloads stay unchanged. One owner releases the session on close or page reload. Check: legacy serialization tests; pending, generation and completion state tests.
2. Publish inventory first, then selected-file content, then bounded enrichment. Tag every task `interactive` or `enrichment`. Check: a deferred-enrichment fixture shows selectable inventory and content before completion; the option matrix converges exactly. Trap: publishing raw Git statuses as final differences; caching handles or `Prepared` to speed up early content.
3. Add batching, backpressure, completion and cancellation. Old generations cannot publish after refresh, close or a root change. Check: delayed, out-of-order, cancelled, closed, consumer-less and failed-session fixtures; resource counts return to baseline; no flood on the 20,000-file fixture; a file open and a ref switch during enrichment stay within baseline plus the agreed margin on Windows.
4. Prove progressive-final equals legacy-final across all options, Windows first. Check: native early-publication and full-completion traces with caches empty and prewarm off.

## Done when
- Inventory and content are ready before enrichment finishes, and the producer is bounded.
- Interactive requests are served within the agreed first-response target during enrichment.
- Stale generation, ref, root, cancel and cleanup cases are correct on both platforms.

## Gates
- `cd src-tauri && cargo test --offline` (`SKEIN_TEST_TMP` on ext4), `rustfmt --check`, clippy
- Windows VM: serial `cargo test --locked`
- `bun run --bun check`, `bun test src/lib` (legacy IPC unchanged)

## Stop and report if
- 18 is not accepted, authorisation could go stale, semantics drift, work is unbounded, or Windows proof is missing.

## Report
Commit sha, the command and type list, step checks, Windows traces, gate results.
