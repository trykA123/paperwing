# 20 — Progressive comparison in the UI

Status: blocked by 19 (and 18)
Platform: Windows first (WebView2, Defender on), Linux parity
Size: L
Role: ui-builder-high (state and lifecycle work), one writer

## Goal
Changed files appear and can be selected before classification, counts and history finish, with an honest "pending" state and a stable selection. The backend's early publication becomes a visibly faster first result.

## Already done
- Nothing progressive in the UI. `src/lib/compare-state.svelte.ts` has `open`, `refresh`, `loadFiles`, `loadAllFiles`, `cancel` and `close`; `loadAllFiles` assigns results only after every page arrives.
- `src/components/FolderCompare.svelte` treats "loading" as a barrier. Whole-set workers (`src/lib/set-compare.svelte.ts`, `SetCompare.svelte`) close their sessions and drilldown opens new ones.
- `src/components/VirtualList.svelte` exists. Binary content IPC arrives with packet 17 (`src/lib/content-bytes.ts`).

## Decisions
- The contract is `docs/progressive-comparison-contract.md`, sections 8, 9 and 11-13: `openProgressive()` beside today's `open()` (set compare keeps `open()` in this packet), identity `{id, generation}` during enriching, hint labels, filters and "N checking", read-only editor until final with a reload from `EditFile.bytes`, re-select by path after refresh, and the `ui.first-row` timer.
- Pending UI follows `docs/progressive-comparison-contract.md` exactly. Lifecycle stays in the existing owners.
- Pending rows never allow copy or save. Selected-file actions request fresh tickets as the contract says.
- Batches apply at most once per animation frame. Large results use `$state.raw` with whole replacement; rows are keyed by stable id; counts and totals come from Rust (shared with packet 36).
- "Counts unavailable" is shown separately from "pending".
- Keep virtualised lists, two whole-set workers, no-exclusion drilldown and destination-only copy retention.
- Frontend paging alone is never claimed as the speed-up; the gain is measured end to end.

## Scope
- Do: `CompareState` adopting the progressive API; pending and final rows, filters and totals; file view, tickets and set drilldown during enrichment; the legacy fallback.
- Do not: cache, prewarm, redesigned pickers, new editor-state ownership.

## Read first
- `docs/progressive-comparison-contract.md`, `src/lib/api.ts`
- `src/lib/compare-state.svelte.ts`, `src/lib/compare-view.ts`, `src/lib/set-compare.svelte.ts`
- `src/components/FolderCompare.svelte`, `SetCompare.svelte`, `FileCompare.svelte`, `VirtualList.svelte`
- `src/lib/comparison-lifecycle.test.js` (existing lifecycle tests)
- `~/.agents/rules/typescript.md`, `~/.agents/rules/web-ui.md`

## Do not touch
- Rust files (packet 19) and `src/styles/` outside pending-row styles.

## Steps
1. Adopt the start and progress API in `CompareState` with generation, revision and batch-order guards and bounded yields. Errors, completion and legacy fallback stay explicit. Check: `bun test src/lib` with delayed, out-of-order, closed, cancelled and failing batches.
2. Render provisional and final lists, filters and summaries with the accepted statuses. Keep the selected id and focus during enrichment. Check: option-matrix fixtures (different ids with equal normalised text, binary, unavailable, type, budget); the first selectable row appears before completion.
3. Connect file viewing, tickets and set drilldown to pending and completion without weakening dirty guards or leaking sessions. Check: open a file during enrichment; refresh, change refs or close with unsaved edits; cancel set workers; session counts; copy and save refused for pending rows.
4. Measure request, first selectable row, completion, with caches empty and prewarm off, on the Windows VM and on Linux. Check: final UI fingerprints equal the legacy ones; memory and cancellation do not regress.

## Done when
- A cache-empty first-useful-render improvement is measured on Windows with unchanged final fingerprints.
- Stale batches, unsaved editors, close, ref change and worker lifecycle behave correctly.
- Screenshots at 1440 and 1100 px, both themes, show the pending state.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (`--write` if styles changed)
- UI: screenshots at 1440 and 1100 px in both themes; Windows walkthrough

## Stop and report if
- Normalised results change, unsaved edits are lost, totals mislead, or the real render is not evidenced.

## Report
Commit sha, step checks, measurement table, screenshots, gate results.
