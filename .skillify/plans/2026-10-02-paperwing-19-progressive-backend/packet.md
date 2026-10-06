# Packet 19 — Progressive comparison backend

| | |
|---|---|
| Status | Proposed. **Blocked** until packet 18's protocol, data types and authority rules are accepted. Needs its own implementation approval. |
| Weight | Heavy (IPC and authority). |
| Depends on | Accepted 09 (platform paths), 18 (progressive contract). |
| Primary platform | Windows (work use). Linux is secondary evidence. |
| Read first | Accepted progressive contract; comparison Service, session and Job; extracted inventory, text and history modules; `lib.rs`; legacy IPC tests. |

## Goal
The backend publishes the list of changed files as soon as it knows it, then fills in classification, counts and history afterwards. Old clients still get the same final result as today. The UI switches over in packet 20, not here.

## Why
`refresh` waits until **all** preparation is finished before returning anything. Batching in the frontend cannot fix that: the barrier is in the backend.

## Scope
- **In:** a new, additive producer mode with generation-tagged batches; foreground priority in the Git runner.
- **Out:** UI changes, caching, prewarm. New command and type names come from accepted packet 18.

## Requirements
- **R1** — Refs, context and inventory are published before enrichment. Native cache-empty traces show rows and file content become selectable early.
- **R2** — The producer is bounded in work, results and IPC rate; it has backpressure, a defined completion or error, and per-consumer cancellation and cleanup.
- **R3** — Legacy commands return identical final output. Progressive output converges to the same final fingerprint.
- **R4 — Foreground priority.** The Git runner admits work in two classes:
  - `interactive`: anything the user asked for directly (open a file, pick a ref, refresh, start a comparison);
  - `enrichment`: classification, count and history fill-in.

  Interactive work never waits behind queued enrichment. Enrichment uses at most half of the existing semaphore slots and never takes the last free slot. It yields between bounded batches. Packet 22 later adds a lower `speculative` class to this same owner.
- **I1** — Session generation, root, ref and metadata checks stay fresh. Provisional rows and retained producer data never grant write authority.
- **I2** — No hidden global session or handle cache, no per-file event flood, no early "final" summary, no change to normalization or budget semantics.

## Memory
Uses the app-wide budget defined in packet 21: at most 32 MiB in flight per comparison and 64 MiB across all comparisons; batches of at most 500 rows or 1 MiB.

## Steps
1. **Add the data types, commands and state machine** from accepted 18, with stable row IDs. Legacy payloads stay unchanged. One Job/session owner, released on close or page reload.
   - Check: legacy serialization and payload tests; pending, generation and completion state tests; native Cargo and Clippy gates.
2. **Split publication:** inventory first, then selected-file content, then bounded enrichment (classification, counts, history). Tag every task `interactive` or `enrichment` (R4).
   - Check: a deferred-enrichment fixture shows selectable inventory and content before completion; the raw-vs-normalized option matrix converges exactly.
   - Trap: publishing raw Git statuses as final differences, or caching handles or `Prepared` to make early content possible.
3. **Batching, backpressure, completion and cancellation.** Old generations cannot publish after refresh, close or root changes. An interactive request during a large enrichment run is served within the ratified first-response target.
   - Check: delayed, out-of-order, cancelled, closed, consumer-less and failed-session fixtures; resource counts return to baseline; no flood at the 20,000-file fixture; a file open and a ref switch during enrichment stay within baseline plus the ratified margin on native Windows.
4. **Prove equivalence** of progressive-final and legacy-final output, plus process, byte, memory and cancellation behaviour across all baseline options, on Windows first, then Linux.
   - Check: common quality gates; native early-publication and full-completion traces with caches empty and prewarm off; authority and refusal tests on both platforms.

## Done when
- **A1 (native release and fixtures):** inventory and content are ready before enrichment; the producer is bounded; interactive work is not delayed by enrichment. Covers R1, R2, R4, I1, I2.
- **A2 (fixtures and native):** legacy and progressive final outputs are equivalent; stale generation, ref, root, cancel and cleanup cases are correct. Covers R2, R3, I1, I2.

## Authority and rollback
The owner approves the exact accepted IPC and authority contract. One writer works in a dedicated worktree; the main session integrates. Use disposable fixtures only; protect real records and settings. Rollback turns off the new endpoints and producer mode while legacy commands keep working. Cancel and release active producers before rolling back code. No persistent data migration.

Stop on missing packet 18 amendments, stale authorization, semantic drift, unbounded work or missing native proof.

## Revision log
- 2026-10-02: Gated additive backend rollout; the frontend stays final-only until packet 20.
- 2026-10-05: Foreground/enrichment admission classes (R4) moved here from packet 22; memory now from the app-wide budget in packet 21; Windows-first proof. Rewritten in plain format.
