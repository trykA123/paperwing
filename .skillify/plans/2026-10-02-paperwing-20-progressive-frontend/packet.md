# Packet 20 — Progressive comparison in the UI

| | |
|---|---|
| Status | Proposed. Needs owner approval after 19 is accepted. |
| Weight | Heavy (public comparison and authority UI contract). |
| Depends on | Accepted 03 (state boundaries), 04 (UI boundaries), 19 (progressive backend). |
| Primary platform | Windows (work use). Native walkthrough on Windows first, then Linux. |
| Read first | Accepted progressive contract and API; CompareState, SetCompareState, FolderCompare, SetCompare, FileCompare; `compare-view.ts`. |

## Goal
Changed files appear and can be selected before classification, counts and history are complete, with an honest "pending" state and a stable selection. This is where the backend's early publication becomes a visibly faster first result.

## Why
- `loadAllFiles` assigns results only after every page arrives, and FolderCompare treats "loading" as a barrier for the whole view.
- Whole-set workers close their sessions and drilldown opens new ones. Reusing authority between them is forbidden, even when the facts could be reused.

## Scope
- **In:** CompareState adopting the progressive API; rendering pending and final rows, filters and totals; file view, tickets and set drilldown during enrichment.
- **Out:** cache, prewarm, redesigned pickers, new editor-state ownership.

## Requirements
- **R1** — Bounded batches appear promptly, without waiting for `loadAllFiles` or full preparation. Completion and errors are explicit.
- **R2** — Pending statuses, filters and directory/set totals converge to today's exact final results. Selection, focus, whole-set progress and drilldown stay coherent.
- **R3** — Cancel, refresh, close and dirty-editor guards reject obsolete updates and keep active editors alive.
- **I1** — Pending rows never allow copy or save. Selected-file actions request fresh tickets as defined in the contract.
- **I2** — Keep virtualized lists, stable IDs, two whole-set workers, no-exclusion drilldown and destination-only copy retention.

## Decisions
- **D1** — Lifecycle stays in the existing owners; pending UI follows packet 18 exactly.
- **D2** — Frontend paging alone is never claimed as the speed-up; the gain is measured end to end.

## Steps
1. **Adopt the progressive start/progress API in CompareState** with generation, revision and batch-order guards and bounded yields. Errors, completion and the legacy fallback stay explicit.
   - Where: `compare.svelte.ts::{open, refresh, loadFiles, loadAllFiles, cancel, close}` and the API calls.
   - Check: `bun test src/lib` with delayed, out-of-order, closed, cancelled and failing batches and explicit completion markers.
2. **Render provisional and final lists**, filters and summaries with the accepted statuses. Keep the selected stable ID and focus during enrichment. Show "counts unavailable" separately from "pending".
   - Where: FolderCompare, comparison child views, `compare-view.ts`, VirtualList callers.
   - Check: client and native option-matrix fixtures, including different IDs with equal normalized text, binary, unavailable, type and budget cases; the first visible, selectable row appears before optional completion.
3. **Connect file viewing, tickets and whole-set drilldown** to pending and completion without weakening dirty guards or leaking sessions.
   - Where: FileCompare, SetCompareState and SetCompare, tab, editor and copy registries.
   - Check: open a file during enrichment; refresh, change refs or close with unsaved edits; cancel set workers and drilldown; session counts; safe-action eligibility and refusal on Windows, then Linux.
4. **Measure** request → first selectable rendered path → completion, natively, with caches empty and prewarm off. Compare final UI fingerprints, memory and cancellation.
   - Check: frontend, build, Cargo and Clippy gates; native walkthrough on Windows (antivirus on), then Linux; ratified first-useful-result targets.

## Done when
- **A1 (client fixtures and native release):** early selectable render and correct pending/final semantics for every option. Covers R1, R2, I1, I2.
- **A2 (fixtures and native, owner-observed):** stale batches, unsaved editors, close, ref change, editor and set-worker lifecycle are correct. Covers R3, I1, I2.
- **A3 (native release, Windows):** a measurable cache-empty first-useful-render improvement with unchanged final fingerprints. Covers R1, R2, I2.

## Authority and rollback
The owner decides interim presentation and IPC behaviour. One writer works in a dedicated worktree; the main session integrates. No real data is changed during measurements; verify sacrificial recovery before save or copy observations. Rollback returns the frontend to the legacy final-only endpoints and releases progressive producers first.

Stop on changed normalized results, lost unsaved edits, stale authority, misleading totals or missing evidence of the actual render.

## Revision log
- 2026-10-02: Proposed progressive consumer after compatible backend acceptance.
- 2026-10-05: Windows-first native walkthrough and measurement. Rewritten in plain format. No change in scope.
