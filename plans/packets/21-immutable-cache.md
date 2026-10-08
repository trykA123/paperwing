# 21 — Cache for repeat comparisons (immutable Git facts)

Status: blocked by 20 (21a); 21b also needs a Windows write-path proof
Platform: Windows first, Linux parity
Size: L (two parts: 21a read paths, 21b write contexts after hits)
Role: api-builder (gpt-6.1-sol xhigh), one writer

## Goal
Repeat comparisons, file opens and whole-set drilldown reuse pure Git facts and become close to instant. Sessions and write authority are never reused. RAM only.

## Already done
- No fact cache and no budget module (`src-tauri/src/budget.rs` is absent). `Service::refresh` in `src-tauri/src/compare.rs` recomputes `Prepared` each time. `Prepared` holds session contexts and safe roots, so it is not cacheable.
- Metadata caching for listings, refs and trees already exists on the frontend and in `src-tauri/src/github/cache.rs`; see `docs/metadata-caching.md`. This packet does not touch it.
- Packet 34 (SQLite store) is not merged. This packet needs nothing from it: nothing here persists.

## Decisions
- Git objects are immutable, so facts keyed by object id cannot go stale. A timer, a name or a path alone is not identity.
- Every key includes the freshly validated physical object-store identity, the object format, the object ids and an algorithm version. Pair facts also include resolved endpoints, side order, options and backend scope.
- Cold, hit, evicted and bypassed paths give identical final output, each with a fresh session and fresh validation.
- Never cache `Prepared`, safe handles or roots, jobs, tickets, transient failures, editor models or write permission.
- 21a: only read paths use hits (compare, read-only file view, drilldown). Every write context (ticket, save, copy, undo) bypasses the cache and rebuilds from fresh validation.
- 21b: write contexts may consume cache-derived facts but always re-check authority. Windows existing write path first; Linux uses `src-tauri/src/linux_files/`.
- No cache lock is held across I/O or an await. Single-flight with per-waiter cancellation and retry cleanup.
- Branch movement, root or metadata replacement, object pruning or a source-scope change must never bring back obsolete data. A version mismatch counts as a miss.
- Bypass and clear exist for disposable cache entries only, never for settings, repositories or journals.

App-wide memory budget, one owned module `src-tauri/src/budget.rs` that every producer reads. Packets lower these, never raise them without a new ratification. Confirm against the Windows measurements first.

| Consumer | Limit |
|---|---|
| Fact cache (21) | 256 MiB retained; one entry at most 16 MiB (larger bypasses); byte-weighted LRU |
| Progressive producer (19) | 32 MiB in flight per comparison, 64 MiB across all; batches of at most 500 rows or 1 MiB |
| Prewarm (22) | no budget of its own; at most 25 percent of the cache budget |
| Backend process | RSS target at most 600 MiB with a 20,000-file comparison open (WebView2 and Monaco measured separately) |
| Windows low-memory notification | cache shrinks to 25 percent of its budget |

## Scope
- Do: a bounded in-memory store of inventories, derived rows, content and history; key builders; the budget module; read-path integration; 21b later.
- Do not: an on-disk cache, learned heat, a working-tree result cache, anything that grants authority.

## Read first
- `src-tauri/src/compare.rs`, `src-tauri/src/compare/{inventory,text_diff,history}.rs`
- `src-tauri/src/platform.rs` (physical identity), `docs/metadata-caching.md`
- `docs/progressive-comparison-contract.md` (from 18) and packet 17 measurements

## Do not touch
- `files.rs`, `file_guard.rs`, `linux_*` (read only; 21b adds call sites later).

## Steps
1. Define eligible entries and key builders, and ratify the budget table. Check: fixtures for the same object ids with different roots, options, order and object formats; entry types cannot hold handles or tickets.
2. Implement the bounded store: lookup, eviction, oversize bypass, single-flight. Check: budget, eviction, flood, oversize, cancelled-waiter and failure fixtures; measured retained, in-flight and process memory. Trap: evicting an entry while consumers hold unaccounted references and calling that memory freed.
3. 21a: integrate read paths. After fresh ref, context and object-store validation, build a new session, generation and safe roots around each hit. Check: cold, hit and bypass equivalence; ref movement; replaced or pruned repositories; missing refs; external working-tree edits; no automatic fetch; a static check that no write entry point receives a cache value.
4. 21a: measure on Windows (Defender on): commands, bytes and time saved, reported apart from total memory. Check: the agreed warm targets are met.
5. 21b: let write contexts use cache-derived facts with revalidation. Check on Windows first: a hit followed by external source, destination, root or metadata changes; active-editor eviction; stale save, copy and undo refused; generation changes.

## Done when
- Keys are complete and fresh authority surrounds every hit.
- Cold, hit, bypass, prune, replacement and option equivalence hold; for 21b, write safety holds on Windows.
- Budget, cancellation and eviction ownership hold; savings are reproducible.

## Gates
- `cd src-tauri && cargo test --offline`, `rustfmt --check`, clippy; Windows VM serial `cargo test --locked`
- `bun run --bun check`, `bun test src/lib`

## Stop and report if
- Authority would be reused, stale external data or pruned objects reappear, a secret leaks, ownership is unbounded, or 21b lacks a Windows write proof.

## Report
Commit sha, key table, budget values, savings table, gate results.
