# Packet 03 — frontend state responsibility boundaries

**Status:** Proposed; requires approval. **Weight:** Standard. **Depends on:** accepted packet 01.
**Read first:** `src/lib/state.svelte.ts`, `compare.svelte.ts`, `workspace.test.js`, `workspace.ts`, and callers in Sidebar/RefPicker/CompareReferencePicker.

## Outcome and scope
Make AppState an app/navigation/persistence/dirty-buffer coordinator with narrow domain owners. Extraction only: keep current keys, freshness and race behavior until packet 16 deliberately changes them. Proposed paths below do not exist yet.

## Requirements and invariants
- R1: Repository metadata, tree state, Activity and whole-set coordination live in cohesive smaller modules.
- R2: Existing `app` and `CompareState`/`SetCompareState` imports and writable façade properties still work.
- I1: Data and its generation/invalidation/watermark state move together; no competing owners or import cycles.
- I2: Dirty tabs, hidden-but-mounted editors, whole-set two-worker ceiling, cancellation and session teardown retain existing behavior.

## Evidence
- [FACT] Sidebar/tests assign `app.trees`; a read-only forwarding getter would break existing callers.
- [FACT] Current commit history key omits requested branch; fixing it here would mix extraction and behavior change.
- [DECISION] Dependency injection/callbacks are narrow explicit seams; do not import the `app` singleton into every child or invent a general state framework.

## Steps
- P1 [BATCH]: Extract pure `segments`/destination/identity calculations to proposed `src/lib/workspace-paths.ts`; AppState methods forward with explicit workspace/item/set/source inputs. Preserve today's platform assumptions and golden vectors.
  - Depends on: none. Location: `state.svelte.ts::segments`, `dest`, `folderOf`, clash helpers.
  - Verify: `bun test src/lib`; shared Rust layout tests.
  - Trap: correcting Linux separators/lowercasing before packet 09.
- P2 [BATCH]: Extract tree state, generation and loading together to proposed `state/repository-trees.svelte.ts`; preserve writable compatibility façade behavior.
  - Depends on: P1. Location: AppState tree fields and `loadTree`, invalidation callers.
  - Verify: `bun run --bun check`; `bun test src/lib` including late-result/tree assignment tests.
  - Fails if: a callback re-enters a circular singleton import or a stale response now wins.
- P3 [BATCH]: Extract Activity list/clear watermark/merge owner to proposed `state/git-activity.svelte.ts`; keep event subscription lifetime in AppState unless existing ownership proves otherwise.
  - Depends on: P2. Location: AppState Activity fields, merge/refresh/clear methods.
  - Verify: the same frontend gates and native clear/cancel/late-event observation.
- P4 [BATCH]: Extract repo/ref/commit request state and existing lookup behavior to proposed `state/repository-metadata.svelte.ts`; forward every current reader/method. Preserve old bugs in characterization, to fix explicitly in 16.
  - Depends on: P3. Location: AppState repository/list/ref/commit fields, `loadRepos`, `ensureRefs`, `ensureCommits`, `refState`; RefPicker and other readers found by `rg`.
  - Verify: `bun test src/lib`; mocked IPC payload/result and selection fixtures; `bun run --bun check`.
  - Fails if: only the writer moves while a reader still reaches obsolete storage.
- P5 [ISOLATE]: Move `SetCompareState` to proposed `src/lib/set-compare.svelte.ts`, re-export it from `compare.svelte.ts`; preserve active-session ownership, workers and drilldown inputs.
  - Depends on: P4. Location: `compare.svelte.ts::SetCompareState`, state/SetCompare callers.
  - Verify: whole-set worker/cancel/close/no-exclusion drilldown tests; native tab-close and dirty-buffer guards.

## Acceptance
- A1 (static + fixture): import/ownership map, existing façade assignments and all frontend checks pass → R1/R2/I1.
- A2 (fixture + native): tree/Activity races, metadata payloads, hidden tabs and set-worker teardown match baseline → R2/I1/I2.

## Commands
`bun run --bun check`; `bun test src/lib`; `bun run --bun build`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`; `git -c core.whitespace=cr-at-eol diff --check`. Verify each responsibility before the next move.

## Stop and rollback
Stop on changed reactivity/lifetime/keys, test-loader failures or missing client/native evidence. Restore owned moves and façade forwarding only. Cache correctness, platform policy and UI redesign are separate packets.

## Revision log
- 2026-10-02: Proposed extraction with rune-loader dependency explicit.

- [REV 2026-10-03] P5 uses compare.svelte.ts as a transparent export façade for CompareState and SetCompareState. CompareState moves unchanged to compare-state.svelte.ts so set-compare.svelte.ts can import it without the façade↔child cycle that a direct extraction would create. Constructor signatures, class identity, lifecycle and public import paths remain unchanged; only the existing pure problem helper is shared between these two owners.
