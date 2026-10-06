# 27 — Release snapshots

Status: blocked by 34 (local store), 26 (restore refuses dirty repositories) and 06s (compare view)
Platform: Windows first, Linux parity (Git CLI only)
Size: M
Role: both (backend first)

## Goal
The user saves the exact commit of every repository in a set as a named snapshot. Later the user restores the set to it, compares the set against it, or shares it as a JSON manifest.

## Already done
- Nothing for snapshots. The word appears in `src/lib/set-compare.svelte.ts` and tests only as a view of compare state.
- Building blocks exist: per-repository status (`LocalStatus` in `src/lib/api.ts`: branch, sha, dirty, upstream), set compare (`src/lib/set-compare.svelte.ts`, `src/components/SetCompare.svelte`), plain branch switch through `startClone(..., 'switch')`.
- Packet 34 adds the SQLite store (`src-tauri/src/store/`, branch `feat/local-store`, not merged at the time of writing).

## Decisions
- Snapshots live in the 34 store, not in settings JSON. Snapshot data is a record of what was true, never authority: restore re-validates everything.
- Per repository the snapshot records: set item id, remote URL (normalised, host kept as written), branch, full commit id, dirty flag, and a snapshot format version.
- Restore checks out the commit detached, or on a new branch the user names. It refuses a dirty repository unless the user stashes first (26 flow).
- Restore never fetches silently. A missing commit is reported with a "Fetch" action.
- JSON export carries no local paths and no credentials.

## Scope
- Do: save, list, rename, delete, restore, compare, export, import.
- Do not: restore through any path other than the existing Git runner; store file contents; auto-schedule snapshots.

## Read first
- `plans/packets/34-*.md` or `src-tauri/src/store/` once merged
- `src/lib/set-compare.svelte.ts`, `src/components/SetCompare.svelte`, `src/lib/api.ts` (`LocalStatus`, `SetItem`)
- `src-tauri/src/git.rs` and `src-tauri/src/commit.rs` (how commands run Git: fixed argv, no shell)
- `plans/packets/26-stash-switch.md`, `plans/packets/06s-fullscreen-compare.md`

## Steps
1. Store table and Rust commands: `snapshot_save`, `snapshot_list`, `snapshot_delete`, `snapshot_rename`. Check: Rust tests for round trip, version field, and that import rejects unknown versions.
2. Restore command: for each repository verify the commit exists locally (`git cat-file -e`), refuse when dirty, then `git switch --detach <sha>` or `git switch -c <name> <sha>`. Return per-repository results. Check: fixture test with a dirty repository, a missing commit and a clean one.
3. Compare with snapshot: build the left side from the snapshot commits and open set compare. Check: fixture where two branches moved; the compare lists exactly those repositories.
4. UI: Save snapshot (name), Snapshots list in the set header or Compare panel, Restore dialog with the detached or new-branch choice and the per-repository preview. Check: browser test, both themes.
5. Export and import JSON; import maps URLs to set items by normalised remote and reports unmatched entries. Check: round-trip test and a test that GitHub Enterprise hosts and github.com stay distinct.

## Done when
- On fixture repositories: save, move branches, compare shows the difference, restore returns every repository to the saved commit.
- A dirty repository blocks restore with a clear message; a missing commit offers fetch.
- Windows: restore works on the VM including a repository under a junction path.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- 34 changes its schema approach.
- Restoring detached HEAD would lose unpushed commits without warning; the dialog must show them first.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
