# Packet 21 — Cache for repeat comparisons (immutable Git facts)

| | |
|---|---|
| Status | Proposed. Needs owner approval. Delivered in two parts: **21a** (read paths) and **21b** (write contexts after cache hits). |
| Weight | Heavy (cache identity and authority boundary). |
| Depends on | 21a: accepted 12, 16, 17, 20. 21b: 21a plus a native write proof on each platform (Windows: the existing journal/ticket/TxF write path; Linux: accepted 14). |
| Primary platform | Windows (work use). Linux is secondary evidence. |
| Read first | Comparison inventory, content, history, session and write-context owners; native identity contract; metadata epochs; baseline, cold-path and progressive evidence. |

## Goal
Reuse pure Git facts across repeat comparisons, file opens and whole-set drilldown, so repeated work becomes close to instant. Sessions and write authority are never reused. RAM only.

## Why
- Git objects are immutable: an object ID always means the same bytes. Facts keyed by object ID cannot go stale.
- Whole-set drilldown rebuilds comparisons that were just computed.
- `Prepared` holds session contexts and safe roots, so it is **not** a cacheable value.

## Scope
- **In:** a bounded in-memory store of pure facts (inventories, derived rows, content, history) keyed by immutable identity.
- **Out:** on-disk cache, learned "heat", working-tree result cache, caching anything that grants authority.

## Requirements
- **R1 — Keys.** Every key includes the freshly validated physical object-store identity, object format, object IDs and algorithm version. Pair facts also include the resolved endpoints, order, options and backend scope.
- **R2 — Equivalence.** Cold, hit, evicted and bypassed paths produce identical final output, each with a freshly built session and fresh validation.
- **R3 — Bounds.** Retained bytes, entry, producer and in-flight limits and per-consumer cancellation are tested. Values over the size limit bypass the cache without truncating results.
- **R4 — Read/write split (21a).** Only read paths use cache hits: compare, read-only file view and drilldown. Every write context (ticket, save, copy, undo) bypasses the cache and rebuilds its inputs from fresh validation, as today. 21b may later let write contexts consume cache-derived facts, always re-checking authority.
- **I1** — Never cache `Prepared`, safe handles or roots, jobs, tickets, transient failures, editor models or write permission.
- **I2** — A timer, name or path alone is not immutable identity. Branch movement, root or metadata replacement, object pruning or a source-scope change must not bring back obsolete data.

## App-wide memory budget (applies to packets 17–22)
Ratify once, **before packet 19 starts**, and keep it in one owned module (proposed `src-tauri/src/budget.rs`) that every producer reads. Packets may lower these values, never raise them, without a new ratification. Initial values, to confirm or tighten against packet 01 measurements on native Windows:

| Consumer | Limit |
|---|---|
| Fact cache (21) | 256 MiB retained; one entry at most 16 MiB (larger bypasses); byte-weighted LRU |
| Progressive producer (19) | 32 MiB in flight per comparison, 64 MiB across all; batches ≤ 500 rows or 1 MiB |
| Prewarm (22) | no budget of its own; at most 25 % of the cache budget |
| Backend process | RSS target ≤ 600 MiB with a 20,000-file comparison open (WebView2/Monaco measured separately) |
| Windows low-memory notification | cache shrinks to 25 % of its budget |

## Steps
1. **Define eligible entries and key builders**, and ratify the budget above, including shared-ownership accounting and active-session memory outside the cache.
   - Check: independent authority and key review; fixtures for same object IDs with different roots, options, order and object formats; entry types cannot hold handles or tickets.
2. **Implement the bounded store:** lookup, eviction, oversize bypass, and single-flight with per-waiter cancellation and retry cleanup. Never hold a cache lock across I/O or an await.
   - Check: budget, eviction, flood, oversize, cancelled-waiter and failure fixtures; measured retained, in-flight and process memory.
   - Trap: evicting the index while consumers still hold unaccounted references, and reporting that memory as freed.
3. **21a — integrate read paths.** After fresh ref, context and object-store validation, build a new session, generation and safe roots around each hit. Reuse inventories, rows and content only where every semantic input is in the key. Prove statically and with fixtures that no write entry point can receive a cache-derived value.
   - Check: cold/hit/bypass equivalence; branch, tag and ref movement; replaced or pruned repositories; missing refs; external working-tree changes; no automatic fetch.
4. **21a — measure** on native Windows: command, byte and time savings, reported separately from total memory and cold timings.
   - Check: ratified warm targets; full quality gates.
5. **21b — write contexts after hits.** Allow write workflows to consume cache-derived facts, revalidating authority. Prove it on native Windows with the existing write path first; Linux only after accepted 14.
   - Check: cache hit followed by external source, destination, root and metadata changes; active-editor eviction; stale save, copy and undo refused; generation changes; consumer isolation.

## Done when
- **A1 (static and fixtures):** keys are complete and fresh authority surrounds every hit. Covers R1, R2, R4, I1, I2.
- **A2 (native fixtures, Windows first):** cold, hit, bypass, prune, replacement and option equivalence; for 21b also write safety. Covers R2, R4, I1, I2.
- **A3 (fixtures and native release):** budget, cancellation and eviction ownership hold; command and time savings are reproducible. Covers R3, I1, I2.

## Authority and rollback
The owner approves the cache, identity and budget decisions. One writer works in a dedicated worktree; the main session integrates. Use disposable profiles and verified restore for mutation drills. Add bypass and clear for disposable facts only, never for settings, repositories or journals. Rollback turns the cache off and clears owned entries after active consumers release them; a version mismatch counts as a miss.

Stop on authority reuse, stale external data, pruned-object resurrection, secret exposure, unbounded ownership or, for 21b, missing native write proof.

## Revision log
- 2026-10-02: Proposed RAM-first immutable reuse after measured foreground delivery.
- [REV 2026-10-02] Independent review: require completed Linux write integration (14) before native cache-hit stale-authority acceptance.
- 2026-10-05: Split into 21a (read hits, no longer gated on 14) and 21b (write contexts after hits, Windows-first proof). App-wide memory budget for 17–22 defined here. Rewritten in plain format.
