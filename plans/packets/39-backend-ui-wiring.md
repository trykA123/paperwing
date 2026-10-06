# 39 — UI for branch cleanup and set search

Status: ready
Platform: Windows first, Linux parity (Git CLI only)
Size: M
Role: ui-builder

## Goal
The user deletes merged branches (local and remote) from a repository or a whole set after a preview, and searches code across every repository in a set with grouped results. Both backends exist; no UI uses them yet. Stash, tags and pull requests are covered by 26, 33 and 28.

## Already done
- Branch cleanup, `src-tauri/src/branch_cleanup.rs` and `branch_cleanup/`: `merged_branches` (lists local candidates with merged flag, upstream gone, in-worktree, last commit; and merged remote branches), `delete_merged_branches` and `delete_remote_branches`. Deletes require each branch's expected tip and re-check that it is still merged; they never force; protected remote names are refused; one leased push per remote; a per-repository lock. Squash merges are not detected. TypeScript wrappers and types are in `src/lib/api.ts` (`mergedBranches`, `deleteMergedBranches`, `deleteRemoteBranches`, `MergedBranches`, `BranchCandidate`, `BranchOutcome`).
- Search, `src-tauri/src/search*.rs`: `search_start` returns a job id and streams events `search-matches`, `search-repo` and `search-done` (names in `events` in `src/lib/api.ts`); `search_cancel`, `search_cancel_all`, `search_capabilities` (perl regex support). Four searches may run at once. Limits: 200 matches per repository and 2000 overall by default, maximums 2000 and 10000, 500 repositories. Modes fixed, basic, perl; ignore case, whole word, path specs, untracked, context lines, a Git ref per repository. Types `SearchRequest`, `SearchMatch`, `SearchRepoStatus`, `SearchDone` and `api.search*` exist.
- No component calls any of these. The view kind `search` in `src/lib/workspace.ts` is the repository browser, not code search.

## Decisions
- Branch cleanup is a modal reachable from the row menu, the drawer and the bulk bar. For a set it runs repository by repository with per-repository results.
- The preview lists candidates with checkboxes. Default selection: merged and not current and not in a worktree. Branches whose upstream is gone are marked, not preselected. Remote deletion is a separate section with its own confirmation naming the remote and the branches. Protected names never appear selectable.
- The UI states that squash-merged branches are not detected.
- Search opens as a new view kind `codeSearch` (a tab), from the set header, the command palette and Ctrl+Shift+F. It is not a new rail section.
- Results group by repository, then file, with line, column, text and context. Streamed in as events arrive. Show per-repository state (done, skipped, cancelled, failed, truncated) and the cap notice. Clicking a match opens the file at the line through the existing compare or file view; if none can show a single file yet, copy the path and say so.
- Search scope: the selected repositories, or the whole set; optional ref per repository (default working tree). Large sets: results are virtualised and cancelled when the tab closes.

## Scope
- Do: the two UIs, state modules, event subscription, cancellation, tests.
- Do not: change the backends; add replace-in-files; add a rail section.

## Read first
- `src-tauri/src/branch_cleanup.rs`, `src-tauri/src/search.rs`, `src-tauri/src/search_job.rs`, `src-tauri/src/search_service.rs`
- `src/lib/api.ts` (`events`, the types above), `src/lib/workspace.ts` (`View`), `src/App.svelte`, `src/components/Tabs.svelte`
- `src/components/VirtualList.svelte`, `src/components/BranchDialog.svelte`, `src/components/set/BulkBar.svelte`, `src/components/CommandPalette.svelte`

## Do not touch
- Stash, tag and PR UI files (26, 33, 28); the compare views (06s, 37).

## Steps
1. `src/lib/branch-cleanup.ts`: load candidates, selection defaults, set runner with per-repository results. Check: unit tests for defaults, expected-tip passing, and one failure among three.
2. Branch cleanup modal and entry points. Check: browser test on a fixture with merged, unmerged, current and worktree branches, both themes.
3. Remote section with separate confirmation and protected names. Check: fixture test against a bare remote.
4. `src/lib/search.svelte.ts`: start, subscribe to the three events, ignore events of other job ids, cancel on close, handle the four-search limit error. Check: unit test with a fake event source (out-of-order events, cancellation, cap).
5. Code search view: query bar (mode, case, whole word, paths, context), grouped virtualised results, per-repository status, open-at-line. Check: browser test on three fixture repositories; 10000 matches stay smooth.
6. Entry points and Ctrl+Shift+F; add the view to the tab and workspace migration. Check: workspace tests still pass, including a saved workspace without the new kind.

## Done when
- A fixture set of three repositories: cleanup removes only the selected merged branches, reports one expected failure, and never offers the current branch.
- A search over the set shows grouped results as they stream and cancels cleanly; closing the tab leaves no running job.
- Windows: both features run on the VM build, including a path with spaces.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- A flow needs a backend change.
- Event volume makes the UI drop frames even with virtualisation.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
