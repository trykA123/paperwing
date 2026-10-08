# 49 — Compare: the branch list is ready when you open it

Status: ready
Platform: both
Size: S
Role: ui-builder (a check from the api-builder side only if step 1 proves a backend cause)

## Goal
In Compare, choosing **Remote branch** (or Local branch, Tag) shows the references immediately. Nobody has to press the refresh button beside the list to make them appear. A list that is really empty says so and says what to do.

## What the owner saw
Left side `master` (local), right side Remote branch: the dropdown opened with nothing in it until the **Refresh reference list** button was pressed. The owner wants this fast and automatic.

## Already done
- `src/components/CompareReferencePicker.svelte`: kind select, `RefSelect`, and a refresh button that calls `app.loadTree(path, true)`. An `$effect` loads each path once with `force = false` when `paths` changes.
- `src/lib/compare-refs.ts` `refChoices` builds the list from `tree.branches`, `tree.tags` and `tree.remotes`; with no loaded tree it returns an empty list and the picker shows no hint.
- `src/lib/state/repository-trees.svelte.ts` `loadTree`: a non-forced call returns at once when a tree with the same identity is cached, so a tree loaded before `git fetch` or before the remote branches existed is reused until the refresh button forces it. `settled` also deletes an entry that is still `loading` when its request disappears; nothing then re-requests it while `paths` is unchanged.

## Likely causes (prove which one is real in step 1; do not guess)
1. Stale cached tree: remote-tracking refs created by a fetch, clone or pull are not in the cached tree and nothing invalidates it.
2. The unforced load was aborted or its entry deleted in `settled`, and the effect does not run again because `paths` did not change.
3. The tree is loaded for the wrong paths (for example the right endpoint's repository is not in `paths` when the kind changes).
4. The repository has no remote-tracking refs (never fetched, or `--single-branch`), so the list is truly empty and the picker gives no explanation.

## Decisions
- No refresh button needed for normal use. Keep it, but as a manual re-read.
- Opening a Compare tab, and changing the kind to a ref kind, requests the tree (stale-while-revalidate: show cached refs at once, re-read in the background, merge when it arrives).
- A tree is marked stale by every Git operation that moves refs in the app (fetch, pull, clone, switch, branch create or delete) and on window focus (`markMetadataStale` exists; check it reaches `RepositoryTrees`).
- An empty list shows a one-line reason in the dropdown: "No remote branches yet. Fetch this repository" with a Fetch action, or "Loading references…" while loading.

## Scope
- Do: reproduce, fix the cause, add the empty and loading states, prefetch on tab open.
- Do not: change the compare engine, the ref model, or `RefSelect` styling beyond the empty state.

## Read first
`src/components/CompareReferencePicker.svelte`, `src/components/RefSelect.svelte`, `src/lib/compare-refs.ts`, `src/lib/state/repository-trees.svelte.ts`, `src/lib/state.svelte.ts` (`markMetadataStale`, `loadTree`), `src/components/FolderCompare.svelte`.

## Steps
1. Reproduce with a browser-harness test (`scripts/testing/browser/`): a repository whose tree was loaded, then gains a remote branch via a mocked fetch; open Compare, choose Remote branch. Check: the test fails today and names which of the four causes applies.
2. Fix that cause. Prefer invalidating the tree on ref-moving operations over polling. Check: the test from step 1 passes without pressing refresh.
3. Prefetch trees for both endpoints when the Compare tab opens or the repository changes, and re-request on kind change. Check: unit test that the first open of the dropdown never renders an empty list while a load is pending.
4. Empty and loading states in the dropdown, with the Fetch action. Check: screenshots at 1440 and 1100 px, both themes, for loading, empty and populated.

## Done when
- Left `master`, right Remote branch lists the remote branches on the first open, with no refresh press, also right after a fetch.
- A repository with no remote refs says why and offers Fetch.
- The refresh button still works and is never required.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts`
- Browser harness for compare; screenshots at 1440 and 1100 px, both themes

## Stop and report if
Step 1 shows the cause is in `repository_tree` on the Rust side (missing remote refs in the response). Report it; that needs an api-builder.

## Report
Commit sha, which cause was real, files changed, each step's check result, screenshots, anything skipped.
