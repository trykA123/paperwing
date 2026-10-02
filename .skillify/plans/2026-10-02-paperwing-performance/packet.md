# PaperWing Performance Plan

Status: Proposed. Planning only; implementation needs approval.
Weight: Standard, with isolated cache-authority and write-safety gates.
Owner: User approves scope and target revisions. One product-code writer preserves concurrent UX changes. No automatic commits, pushes, or releases.

## Outcome And Scope

Make set navigation, ref selection, repeated comparisons, and file opening responsive using Rust, Svelte 5, and Bun. Include fresh standalone-executable acceptance. Preserve all existing checkout, comparison, save/copy, and recovery semantics.

In: measurements, metadata freshness, deduplicated requests, bounded prefetch, immutable Git caches, proven compute/I/O hot paths, incremental UI delivery.
Out: visual redesign, new databases, automatic fetch/push, relaxed filesystem safety, credential caching, and persistent source-content/diff caches. Sets remain persisted configuration.

## Evidence

- F1 [FACT]: `src/lib/state.svelte.ts` loads saved sets, caches refs by URL and local trees by path. Commit picker history is keyed by repository ID although requests include a branch.
- F2 [FACT]: `src-tauri/src/github.rs` reuses repository listings from a source-ID disk cache without a TTL/configuration-fingerprint gate.
- F3 [FACT]: `src/lib/compare.svelte.ts` replaces comparison sessions and accumulates every file page before assigning the list. Whole-set workers close row sessions; drilldown creates another comparison.
- F4 [FACT]: `src-tauri/src/compare.rs` -> `Service::refresh` recomputes prepared results. `Prepared` contains session-specific contexts and safe handles; it is not a reusable global cache value.
- F5 [FACT]: `line_counts` writes temporary files and launches Git per invocation. Directory aggregation repeatedly scans rows for direct children. Their dominance is unmeasured.
- F6 [FACT]: `FileCompare.svelte` loads Monaco lazily. The current picker uses shared `Select`, `RefSelect`, and `compare-refs.ts`; preserve this UX work.
- H1 [ASSUMPTION]: immutable reuse and fewer per-file processes offer the largest gains. Confirm or reject through P1 timings and command counts before implementing expensive rewrites.

## Stack Responsibilities

- Rust owns Git plumbing, ref/context validation, inventory, normalization, immutable caches, and expensive processing. Use batched object reads and shared pure values (`Arc`). Keep locks out of I/O awaits. Move expensive synchronous work to bounded blocking workers; evaluate Rayon only when CPU measurements justify it.
- Svelte owns responsive presentation: scoped reactive state, inexpensive derived visible views, virtualized lists, bounded result batches, focus/selection preservation, and generation guards. Keep Monaco lazy unless measured idle warming improves first-open latency without hurting startup.
- Bun owns fixture orchestration, frontend tests, benchmark reports, builds, and local packaging. It is not an added runtime requirement for desktop users. Reuse existing test files; run independent frontend checks in parallel and timing-sensitive Rust checks serially.

## Requirements And Invariants

- R1: Loaded set navigation/filtering performs no remote request or full recomputation.
- R2: Metadata requests deduplicate; commit histories are branch-scoped, retryable, and invalidated correctly.
- R3: Immutable comparisons reuse pure Git-derived work after fresh ref resolution and context validation.
- R4: Ref/source changes, repository replacement, external file edits, and obsolete responses never masquerade as fresh results.
- R5: Cache residency, producers, speculative requests, and CPU work have tested bounds.
- R6: Cold operations and result delivery improve measurably while remaining cancellable.
- R7: Deliver packaged desktop evidence and a new local executable; preserve release assets/history.
- I1: Fresh sessions receive fresh IDs, generations, root/metadata validation, and safe handles. Cached bytes or display rows never authorize writes.
- I2: Keep every save/copy/undo ticket, dirty-buffer, fingerprint, containment, backup, and recovery check unchanged.
- I3: Preserve binary/unavailable reporting, EOL/whitespace rules, modes, submodules, history, and destination-only retention semantics.
- I4: Cancellation/eviction/close must not invalidate another consumer or active editor. Logs contain no credentials/file contents. Prefetch performs no fetch, checkout, stage, commit, or push.

## Targets And Measurement Contract

Targets are proposals, not measured claims. P1 fixes reference hardware and fixtures; changing a target requires user approval supported by measurements.

| Operation | Proposed target |
|---|---|
| Saved-set switch / loaded filtering | p95 <= 100 ms for 1,000 rows; zero remote calls |
| Warm ref picker | p95 <= 100 ms; no duplicate listing requests |
| Repeat immutable comparison | p95 <= 500 ms on 5,000 files; >= 80% improvement; no repeat blob reads/line-count processes |
| Warm file diff | p95 <= 300 ms for supported 100 KiB text after Monaco is loaded |
| Cold comparison | >= 30% median improvement in P1's proven dominant case |
| Cancellation | p95 <= 250 ms on the reference fixture |

Use release desktop builds. Separate process-cold, app-warm/cache-empty, and cache-hit measurements; restarting an app does not clear the OS disk cache. Record p50/p95, five warm-ups plus twenty short-operation samples, ten expensive cold samples, hardware/tool versions, source inputs, IPC bytes, subprocess counts, retained cache bytes, total process memory, and UI tasks longer than 50 ms. Separate network delays from local work.

Fixtures: 1,000 set rows, large ref lists, 5,000-file mostly-identical and heavily-changed repositories, supported text, binary/type-conflict/submodule cases, duplicate checkouts, missing refs, externally changed files/refs. Use disposable repositories.

## Cache And Scheduling Policy

- Listings: version existing disk entries; key by source configuration fingerprint and nonsecret credential revision. Show valid cached data immediately; revalidate after five minutes. Source/token edits invalidate scope; permission failures make stale entries unavailable.
- Remote refs/local trees: 30-second TTL, force refresh bypass, relevant Git-operation/focus invalidation. Keys include source scope or validated local metadata identity respectively.
- Commit history: key by source scope, repository, requested branch, and ref epoch; 30-second TTL, retry after errors. Update all readers together with the writer.
- Immutable inventory/blob cache: validated object-store identity, object ID/object format, algorithm version. Pair results additionally include both resolved commits, side order, comparison options and any backend path scope. Resolve branch/tag names for every explicit Compare.
- Cache pure facts, never `Prepared`, pinned roots, jobs, tickets, or transient failures. Construct newly validated sessions around hits. In-flight deduplication has per-waiter cancellation and retryable failure cleanup.
- Start with 128 MiB retained cache (16 MiB metadata, 112 MiB immutable), weighted LRU, 64 metadata repositories and 128 pair results. Account for arrays/strings/shared ownership; measure in-flight/editor memory separately. Oversized entries bypass caching without truncation.
- Preserve the current four-work-slot comparison ceiling until measurements justify change. Prefetch at most two local metadata requests within a foreground-prioritized budget, only for active visible/focused rows, after first render. Drop obsolete queued work. Cap CPU workers at min(available parallelism, 4) initially.
- Coalesce generation-tagged progress to at most ten updates per second. Avoid one event per file or full-snapshot events. Benchmark binary IPC for file bytes only if JSON transfer is a proven bottleneck.

## Ordered Plan

### P1 [ISOLATE]: Baseline

Location: `compare.rs` -> `Job`, `prepare`, `inventory`, `line_counts`; `compare.svelte.ts` load/open; `FileCompare.svelte` editor initialization. Add development/test-only monotonic phase timing and counts, reusing Git Activity. Verify using the fixture matrix and existing scaled inventory test. Done when hardware/targets and the top two measured costs are documented. Stop on noisy/unattributable measurements.

### P2 [BATCH]: Metadata Freshness And Deduplication

Depends on P1. Location: `state.svelte.ts` loaders/invalidation, `RefPicker.svelte`, `CompareReferencePicker.svelte`, `github.rs` listing cache, and bounded discovery of Settings source/token-save handlers. Apply Cache Policy; preserve user selection during refresh. Tests cover branch switch, concurrent requests, retries, TTL, force refresh, source edits and late responses. Done when R1/R2/R4 pass. Trap: update every commit-cache reader, including `refState`, with its key.

### P3 [BATCH]: Conservative Prefetch

Depends on P2. Location: active context/loaders, `SetView.svelte` visible range, picker paths. Apply scheduling policy with foreground promotion and obsolete-queue cancellation. Verify bounds, no fetch, no inactive-set work, and startup/selector enabled-versus-disabled timings. Keep it disabled if foreground responsiveness regresses.

### P4 [ISOLATE]: Immutable Rust Cache

Depends on P1/P2. Location: `compare.rs` service/prepare/inventory/content/history; add focused `compare_cache.rs` only if needed. Share pure values with fresh authority/session validation. Verify cold/warm equivalence, command savings, ref movement, replaced roots/metadata, pruned objects, options, side order, eviction, oversized bypass and cancelled waiters. Done when R3/R4/R5 and I1-I4 hold. Trap: sharing `Prepared` reuses authority-bearing state.

### P5 [ISOLATE]: Proven Rust Hot Paths

Depends on P1/P4. Location: directory aggregation, `blobs`, `line_counts`, normalization and cancellation loops. Use bottom-up indexed child aggregation; batch immutable reads; reuse existing Git diff metadata where equivalent. If CPU diff work dominates, benchmark a maintained Rust diff library and bounded workers against Git on adversarial fixtures. Adopt Rayon only with measured benefit. Done when cold targets and exact result-equivalence gates pass; stop rather than silently change diff semantics.

### P6 [ISOLATE]: Progressive Svelte Delivery And IPC

Depends on P4/P5. Location: file-page delivery, set workers/drilldown, FolderCompare/SetCompare/FileCompare, and typed comparison IPC. Yield between bounded batches; distinguish pending/final totals; expose real preparation phases if needed. Reuse immutable facts on fresh-session drilldown and immutable file reopening, never writable tickets/models. Verify obsolete pages, cancel/teardown, dirty guards, unchanged final summaries, options/exclusions and first/warm editor loading. UI pagination alone is not a backend speedup.

### P7 [ISOLATE]: Safety And Packaged Acceptance

Depends on P2-P6. Extend existing tests for cache hits followed by external edits/ref movement/root replacement and active-editor eviction. Add a cache-bypass/clear test path for disposable entries only. Run final measurements and native quality gates; build and launch a fresh standalone executable in a new ignored test folder. Record input revision/diff, binary header and SHA-256, then obtain user interaction acceptance. Existing releases/history stay unchanged.

## Acceptance

- A1: before/after desktop report meets Targets -> R1/R6/R7.
- A2: metadata scopes, freshness, retries and dedup fixtures pass -> R2/R4/I4.
- A3: warm hits match cold output, resolve names and validate authority -> R3/R4/I1/I3.
- A4: eviction, flood, cancel and close obey memory/work/ownership limits -> R5/R6/I4.
- A5: external-change save/copy/undo tests retain existing refusal/recovery behavior -> R4/I1/I2/I3.
- A6: partial results never look final; stale results cannot replace active state -> R6/I3/I4.
- A7: packaged walkthrough and test executable/hash recorded; release files/history unchanged -> R7/I4.

```powershell
rtk bun run --bun check
rtk bun test src/lib/workspace.test.js src/lib/workspace.test.ts
rtk proxy cargo test --manifest-path src-tauri/Cargo.toml --lib scaled_working_inventory_is_lazy_bounded_and_cancellable -- --test-threads=1 --nocapture
rtk proxy cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
rtk proxy cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
rtk bun run --bun tauri build --no-bundle
rtk proxy git -c core.whitespace=cr-at-eol diff --check
```

Review P1 baseline, P2/P3 metadata, P4 authority, P5 compute, P6 delivery and P7 packaged evidence separately. Keep intentionally ignored destructive tests controlled. Reuse existing fixtures; add a benchmark harness only where required.

## Stop Conditions And Rollback

Stop on unsafe authority reuse, semantic drift, unbounded work/memory, ambiguous cancellation, private-data leakage, flaky timings, concurrent writer collision or unjustified target misses. Amend the plan and obtain approval. Rollback disables/clears disposable caches, never settings, repositories or recovery records; version mismatch is a miss. Preserve all unrelated edits.

Revision: 2026-10-02 initial proposal saved with the source-only personal copy. No performance implementation or baseline claimed.