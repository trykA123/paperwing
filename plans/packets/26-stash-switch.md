# 26 — Stash UI and switch with stash across a set

Status: ready
Platform: Windows first, Linux parity (Git CLI only, no platform code)
Size: M
Role: ui-builder (a small api-builder step first for the set-wide runner)

## Goal
The user can stash, apply, pop and drop stashes in one repository from the drawer. The user can switch a whole set to another branch when some repositories are dirty: Skein stashes, switches, and offers to restore each stash.

## Already done
- Backend per repository, registered in `src-tauri/src/lib.rs`: `stash_list`, `stash_push` (message, include untracked), `stash_apply`, `stash_pop`, `stash_drop`, `stash_show`, `switch_with_stash` in `src-tauri/src/stash/commands.rs`, `ops.rs` and `switch.rs`.
- Outcomes: `PushOutcome`, `ApplyOutcome` (applied, stashKept, indexRestored, conflicted, error), `StashDiff` (patch, truncated), `SwitchOutcome` (stashed, switched, error). A conflicting apply or pop keeps the stash. A per-repository lock serializes calls. The commands refuse to run while a clone, fetch or pull is running.
- TypeScript types exist in `src/lib/api.ts` (`StashEntry`, `StashPushOutcome`, `StashRestoreOutcome`, `StashDiff`, `SwitchStashOutcome`). The `api` wrappers do not exist.
- Missing: api wrappers, any UI, and the set-wide runner. The bulk bar already has a plain "Switch" (`src/components/set/BulkBar.svelte`, `startClone(..., 'switch')`) that does not stash.

## Decisions
- Never drop a stash automatically. Drop is a separate action with a confirmation naming the stash.
- A set-wide switch runs repository by repository (not in parallel on one repository) with per-repository results. One failure does not stop the rest.
- After a switch, Skein shows one "Restore stashes" step listing each stashed repository. Restoring uses apply (keeps the stash if it conflicts), then offers drop only after a clean apply.
- Remember the stash oid returned by `switch_with_stash`; never address a stash by index.

## Scope
- Do: api wrappers; stash list with diff preview in the repository drawer; per-repository and bulk stash, apply, pop; "Switch with stash" in the bulk bar and row menu; the restore step; per-repository result list.
- Do not: change the stash backend semantics; auto-drop; stash during a running clone, fetch or pull.

## Read first
- `src-tauri/src/stash/commands.rs`, `switch.rs`, `ops.rs`
- `src/lib/api.ts` (stash types), `src/components/set/BulkBar.svelte`, `src/components/set/RowMenu.svelte`
- `src/components/HistoryDrawer.svelte`, `src/components/right/RepositoryTree.svelte` (shows stash names today)
- `src/lib/state.svelte.ts` (`startClone` and how bulk actions report results)

## Do not touch
- `src/components/CommitDialog.svelte` and discard code (29 owns them).

## Steps
1. Add the `api` wrappers for the seven commands. Check: `bun run --bun check` and a unit test that each wrapper sends the right command name and arguments.
2. Add a set-wide switch runner in `src/lib` that loops repositories, calls `switch_with_stash`, and collects `SwitchOutcome` per repository. Check: unit test with a fake API for success, dirty with stash, switch failure, and one failure among three.
3. Drawer "Stashes" section: list, message, date, branch; diff preview through `stash_show` (show the truncation notice); Stash changes, Apply, Pop, Drop with confirmation. Check: browser test against a fixture repository.
4. Bulk bar and row menu: "Switch with stash" and "Stash changes…" (message, untracked option). Result dialog lists each repository and its outcome; the restore step follows. Check: browser test on a three-repository fixture, two dirty.
5. Forced conflict: the stash stays, the conflicted files are listed, and Skein offers "Open repository". Check: fixture test.

## Done when
- A fixture set of three repositories, two dirty, switches branch and restores both stashes.
- A forced conflict keeps the stash and shows the conflict.
- Windows: stash with untracked files and long paths works on the VM; results are the same as on Linux.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline stash` (only if Rust changed; set `SKEIN_TEST_TMP` to an ext4 directory)
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- A UI flow needs a backend change beyond a thin wrapper.
- A restore can lose work (an apply that drops the stash on conflict).

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
