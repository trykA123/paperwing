# Packet 22 — Optional local prewarming

| | |
|---|---|
| Status | Proposed, **optional**. Needs explicit approval and a measured remaining benefit. |
| Weight | Standard. |
| Depends on | Accepted 11 (Git lifecycle), 16 (metadata single-flight), 19 (priority classes), 20 (progressive UI), 21a (fact cache). |
| Primary platform | Windows (work use). A benefit must show on native Windows. |
| Read first | Cold-path and cache evidence; metadata single-flight; Git runner ownership and packet 19 admission classes; SetView visible context; active picker and compare state. |

## Goal
If it pays off, quietly prepare local facts for the repositories you are looking at, after the first screen has rendered, without ever slowing down what you are doing. Leaving this packet out is acceptable.

## Scope
- **In:** local, read-only preparation for the active set, visible repositories and focused repository.
- **Out:** a manual "Warm" button, learned heat, crawling the whole catalog, a disk blob cache, any remote read or fetch, any mutation.

## Requirements
- **R1** — Work is limited to what the user is looking at, starts after the first useful render, and stays within a small ratified concurrency budget.
- **R2** — A foreground request joins or promotes matching speculative work and never waits behind unrelated speculation. Obsolete or consumer-less work is dropped safely.
- **R3** — Enabled-versus-disabled native release runs on Windows show a benefit with no regression in startup, cold comparison, cancellation or memory.
- **I1** — No automatic `fetch`, `ls-remote`, missing-ref acquisition, checkout, stage, commit, push or credential access. Only proven local immutable or metadata operations.
- **I2** — Cancelling one speculative consumer never cancels a foreground consumer or invalidates an editor or session.

## Decisions
- **D1 — Scheduler:** reuse the admission owner from packet 19 (R4). This packet only adds a `speculative` class below `enrichment`, so speculation can never delay interactive or enrichment work. Candidates: two speculative jobs and four CPU slots, to be measured, not assumed.
- **D2 — Memory:** no budget of its own; at most 25 % of the app-wide cache budget in packet 21.
- **D3 — Off by default** until the owner approves a measured result. A disable switch always exists.

## Steps
1. **Find a measured workload** that local prewarm improves (navigation or file open). Ratify the budget and task list. Audit each task's full call path for hidden network use.
   - Check: recorded traces and request counts; an exact local-only allowlist.
   - Stop if: no benefit, or any task can reach the network.
2. **Add the `speculative` class** to the packet 19 admission owner, with foreground promotion, coalescing of visible context and removal of obsolete queued work. Keep it disabled outside the measurement path.
   - Check: concurrency, flood, promotion, no-consumer and cancel fixtures; zero remote commands or API requests.
   - Trap: treating Git's global 32 slots as an extra, independent background budget.
3. **Integrate** post-first-render preparation for the active context. Compare enabled and disabled native release runs on Windows (real-time antivirus on). Enable by default only if the owner approves the result.
   - Check: common quality gates; startup, cold, open, cancel and memory distributions; foreground-under-speculation fixtures on Windows, then Linux.

## Done when
- **A1 (fixtures):** bounded local admission, zero speculative network or mutation, correct promotion and cancel isolation. Covers R1, R2, I1, I2.
- **A2 (native release, Windows):** a reproducible benefit with no foreground regression, or a documented omission or disabled status. Covers R3, I1, I2.

## Stop and rollback
Stop on hidden network use, work for the wrong set, any foreground regression or unbounded ownership. Rollback disables the speculative class and clears only queued or owned speculative work; foreground producers, sessions, cache facts and user data stay intact. Do not invent heat mechanisms to rescue an unproven prewarm.

## Revision log
- 2026-10-02: Optional, evidence-gated local speculation, last and never ahead of foreground responsiveness.
- 2026-10-05: Scheduler moved to packet 19; memory shares the packet 21 budget; benefit must be shown on native Windows. Rewritten in plain format.
