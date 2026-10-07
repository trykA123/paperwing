# 33 — Tags UI, set-wide tagging and GitHub release option

Status: ready
Platform: Windows first, Linux parity
Size: M
Role: ui-builder, with a small api-builder step for the set runner and the release call

## Goal
The user creates, pushes and deletes tags for one repository or a whole set in one dialog, sees tags in the drawer and ref picker, and can create a GitHub release from a pushed annotated tag.

## Already done
- Backend in `src-tauri/src/tags.rs`, registered in `src-tauri/src/lib.rs`: `list_tags`, `create_tag`, `push_tag`, `delete_tag`, `delete_remote_tag`, with tests in `src-tauri/src/tags/tests.rs`.
- Rules already enforced: names checked by `valid_ref`; a missing message is refused when the repository signs tags; annotated tags use `-a -m`; `create_tag` refuses an existing name unless `moveExisting`; `push_tag` pushes exactly `refs/tags/<name>` and moves a remote tag only with an expected-object lease; `delete_tag` is local only; `delete_remote_tag` is separate. Fixed argv, no shell, never `--tags`.
- `api.listTags`, `api.createTag`, `api.pushTag`, `api.deleteTag`, `api.deleteRemoteTag` and the types (`TagInfo`, `CreateTagRequest`, `CreatedTag`, `PushedTag`, `DeletedTag`) exist in `src/lib/api.ts`.
- Tags already show in the ref picker (`src/components/RefPicker.svelte`) and in the repository tree list (`src/components/right/RepositoryTree.svelte`).
- Missing: the tag dialog, the set-wide runner, the history drawer rails, the GitHub release call, ref-epoch bump after a tag change.

## Decisions
- One dialog for one or many repositories. Name and message apply to all; target defaults to each repository's HEAD; the preview lists repository, commit and any existing tag before anything runs.
- Annotated is the default when a message is given. Signing is never configured by Skein.
- Push is an explicit checkbox ("Push after creating"). The remote is chosen per repository, default `origin`, shown in the preview.
- Remote delete needs its own confirmation naming the remote and the repositories. "Move tag" needs a second confirmation and shows old and new commit.
- The GitHub release uses the stored token, the repository's host from its remote and configured source (github.com or Enterprise; never hardcoded), draft by default, notes prefilled from the tag message. It is offered only after a successful push of an annotated tag.
- Per-repository results; one failure does not stop the others.

## Scope
- Do: set runner in `src/lib`; tag dialog; tags in the history drawer rails; ref epoch bump; delete flows; release call and its UI.
- Do not: change the tag backend rules; push all tags; create releases for lightweight tags.

## Read first
- `src-tauri/src/tags.rs`, `src-tauri/src/tags/tests.rs`
- `src/lib/api.ts`, `src/components/BranchDialog.svelte` (dialog pattern), `src/components/HistoryDrawer.svelte`, `src/components/HistoryGraph.svelte`, `src/lib/history-graph.ts`
- `src-tauri/src/github/pulls/source.rs`, `src-tauri/src/github/pulls/client.rs` (how host and token are resolved)
- `src/lib/state/repository-metadata.svelte.ts` (ref epochs)

## Steps
1. Set runner `src/lib/tags-set.ts`: plan (preview rows with existing-tag detection), execute create, push, delete with per-repository results. Check: unit tests with a fake API, including one failure among three and the duplicate refusal.
2. Tag dialog from the bulk bar, row menu and drawer. Check: browser test, both themes, 1440 and 390 px.
3. Drawer rails show tags at their commits; creating or deleting a tag bumps the ref epoch so pickers and caches refresh. Check: unit test for the epoch bump; browser check.
4. Delete flows: local delete, then separate remote delete confirmation. Check: fixture test.
5. GitHub release: Rust command `create_github_release` (path, tag, notes, draft) built on the pulls source and HTTP client, with tests for GitHub Enterprise host resolution, rate limit, and missing permission. Then the dialog option. Check: recorded-response tests; fixture run.

## Done when
- On the `skein-fixture` repositories: tag a set `v2.4.0` annotated, push, see it on GitHub and in the drawer, delete it locally and remotely; a duplicate name is refused; cleanup removes the fixture tags and the release.
- A repository on a GitHub Enterprise host creates its release on that host.
- Windows: the same flow runs on the VM build.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline tags github` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- A UI flow needs a change to the tag backend rules.
- The release call needs a credential scope the stored token cannot have.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.

## Decided 2026-10-07 (review of the UI, steps 1–4)
- Remote tag delete takes a lease. `delete_remote_tag` gains an optional `expected` object id and pushes `--force-with-lease=refs/tags/<name>:<object>`. The UI never submits a repository whose object it does not know. This is a backend change; do it with step 5 on Codex. Tests: a remote tag moved by someone else is refused; a matching object is deleted.
