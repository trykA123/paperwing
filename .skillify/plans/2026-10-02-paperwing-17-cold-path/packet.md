# Packet 17 — Faster first comparison (cold path)

| | |
|---|---|
| Status | Proposed. Needs owner approval. |
| Weight | Standard. Semantic and safety gates run in isolation. |
| Depends on | Accepted 01 (baseline), 02 (comparison modules), 09 (platform paths). |
| Not dependent on | Cache (21) or prewarm (22). |
| Primary platform | Windows (work use). Linux is secondary evidence. |
| Read first | Baseline report; extracted inventory, text-diff and history modules; `Service::prepare`; final-result fixtures. |

## Goal
Make a comparison with empty caches faster by removing measured waste, one bottleneck at a time. Final results stay byte-for-byte identical.

## Why
- `line_counts` creates a temp file and starts a Git process **per file**. On Windows, real-time antivirus also scans every process start and every new temp file, so this cost multiplies.
- Directory totals re-scan rows repeatedly.
- Inventory already uses batched object reads.
- Which of these dominates is **not measured yet**. Measure first.

## Scope
- **In:** the cold foreground comparison path; costs that measurement proves.
- **Out:** caching, prewarm, broad rewrites, changing what a comparison means, replacing Git with a diff library without a separate approval.

## Requirements
- **R1** — The selected bottleneck improves on the ratified fixture, with no worse memory, cancellation or correctness.
- **R2** — Final options, results, counts, history and limits stay identical to the baseline fingerprints.
- **I1** — Fresh context, authorization and cancellation stay as they are. "Counts unavailable" is never shown as "Same".
- **I2** — No automatic network or mutation. No unbounded blocking or CPU work. Existing work ceilings stay until a separate amendment.

## Decisions
- **D1 — Measure on native Windows with real-time antivirus on** (Defender or the managed work equivalent). Record process spawns, temp files and wall time per comparison next to the existing counters.
- **D2 — Candidate order, best first.** Each still needs the exact-semantics gate:
  1. **One `git diff --numstat -z` (or `--raw -z`) per comparison scope** instead of per-file `line_counts`. No temp files. Options that change normalization (EOL, whitespace) must map to equivalent flags; otherwise that file falls back to the current path.
  2. **One long-lived `git cat-file --batch-command` process per repository and Job** for object reads. Owned by the existing runner and semaphore; torn down with the Job under the packet 11 lifecycle rules.
  3. **In-process object reads with gitoxide (`gix`)**, for blob and tree access only. Needs its own dependency approval (step 3). Never used for writes, credentials or network.
- **D3** — Do not implement every candidate. Stop when the measured bottleneck is gone.

## Steps
1. **Isolate the cost.** Pick at most two dominant, separately measurable costs. Record the fixture and target before changing code.
   - Check: instrumented release traces attribute the cost; the owner ratifies the targets.
   - Stop if: no bottleneck is demonstrated.
2. **Fix the first one** in its owning module, following D2.
   - Check: all option and adversarial fixture fingerprints match; serial Cargo tests pass; native before/after numbers for time, process spawns, temp files, bytes and memory.
   - Trap: reusing raw Git statuses or counts where normalization options change what is displayed.
3. **Only if CPU or diff cost still dominates:** benchmark `gix` or bounded worker threads against Git, behind a separate dependency approval.
   - Check: adversarial EOL, whitespace, long-line, binary and type fixtures; cancellation under load; native p50/p95 latency and process memory.
   - Stop if: any result or count differs, cancellation cannot interrupt the work, or foreground contention gets worse.
4. **Replay** the cold workloads with caches empty and prewarm off. Document gains, regressions and remaining assumptions.
   - Check: ratified targets met with distributions; frontend, Cargo, Clippy, build and diff gates pass; native comparison walkthrough.

## Done when
- **A1 (native release, Windows):** the selected bottleneck improves reproducibly; process spawns and temp files per comparison go down; memory and cancellation bounds hold. Covers R1, I2.
- **A2 (fixtures and native):** final output, limits, failures and cancellation match the baseline. Covers R2, I1, I2.

## Stop and rollback
Stop on unproven dominance, semantic drift, an unjustified dependency or any regression. Roll back only this packet's optimization code; keep the benchmark and equivalence evidence. A cache, prewarm or looser target cannot hide a failed cold change.

## Revision log
- 2026-10-02: Proposed evidence-selected optimizations, not a mandate to replace Git.
- 2026-10-05: Windows-first measurement with antivirus on; candidate order numstat batch, then persistent cat-file, then `gix` (separately approved). Rewritten in plain format.
