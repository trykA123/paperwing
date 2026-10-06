# 28 — Pull request UI: status column, open dialog, bulk open

Status: ready
Platform: Windows first, Linux parity
Size: M
Role: ui-builder (a small api-builder step for the batch runner and the optional CI status cache)

## Goal
The user sees the pull request and check status of each repository's current branch in the table, opens a pull request from a branch, and opens pull requests for a whole set branch in one step.

## Already done
- GitHub client in `src-tauri/src/github/pulls/` (`client.rs`, `model.rs`, `repository.rs`, `source.rs`, `summary.rs`, `error.rs`). Commands in `src-tauri/src/github/pulls.rs`, registered in `src-tauri/src/lib.rs`: `pull_for_branch` (path, branch -> `Option<PullRequest>`) and `open_pull_request` (path, request -> `CreatedPullRequest`).
- Host and source are resolved from the repository's remote and the configured sources (`pulls/source.rs`): github.com, GitHub Enterprise hosts and manual sources work, and a missing source gives "Add a source for <host>". The token comes from the stored credential only.
- Errors: `PullsError` is `rateLimited` (with `resetAt`) or `message`. Tests cover GHES, rate limits and merged state.
- TypeScript: `PullRequest`, `OpenPullRequest`, `CreatedPullRequest`, `PullsError` and `api.pullForBranch`, `api.openPullRequest` exist in `src/lib/api.ts`, with `src/lib/api-pulls.test.js`.
- Missing: every UI, the batch runner, a per-row PR cache, and the rail badge.

## Decisions
- No new credential storage and no new GitHub client. Use the two commands as they are.
- Fetch the PR for the current branch only, on demand and on refresh, with a concurrency limit (4) and a per-session cache. No polling.
- Rate limit: show the reset time and stop the batch. Never retry in a loop.
- Orgs can have more than 100 repositories: the PR column loads lazily for visible rows plus selected rows, never for the whole set at once.
- Bulk open never pushes. A repository with `hasUnpushedCommits` or no remote branch is reported and skipped unless the user chose "Push first" (existing `push_branch`).

## Scope
- Do: PR column in the Formation table; PR chip with state, review and checks; open-PR dialog; bulk open with per-repository results; rail badge for "to review" once data exists.
- Do not: merge, review, comment or close PRs; new Rust GitHub code beyond a thin batch helper if needed.

## Read first
- `src-tauri/src/github/pulls.rs`, `pulls/model.rs`, `pulls/source.rs`
- `src/lib/api.ts` (pull types), `src/lib/api-pulls.test.js`
- `src/components/set/FormationRow.svelte`, `src/lib/formation-row.ts`, `src/lib/formation.ts`, `src/components/set/BulkBar.svelte`
- `src/components/BranchDialog.svelte` (dialog pattern), `src/lib/state/repository-metadata.svelte.ts`

## Do not touch
- `src-tauri/src/github/http*` and credentials code.
- Provider boundaries: 38 may move this client behind `PullRequestProvider`. Keep UI calls in one `src/lib` module so that move changes one file.

## Steps
1. `src/lib/pulls.svelte.ts`: a loader with cache, concurrency limit and rate-limit state around `api.pullForBranch`. Check: unit tests for cache, limit, rate-limit stop and "Add a source" message.
2. Table: PR column and chip (number, draft, open, merged, review, checks), click opens the PR URL in the browser. Check: browser screenshots in both themes at 1440 and 1100 px.
3. Open-PR dialog from the row menu and the drawer: title (default last commit subject), body, base branch (default the repository default), draft flag. Shows the target repository, so forks are visible. Check: browser test with a mocked API; fixture run on `skein-fixture-api`.
4. Bulk "Open pull requests" in the bulk bar: preview table (repository, head, base), per-repository results, links. Check: unit test for skip rules and one failure among three.
5. Register the "to review" count with the rail badge map from 03b once that exists.

## Done when
- On `skein-fixture-api`: push `feature/login`, open a draft PR from Skein, the row shows the PR and its state; cleanup closes the PR.
- A GitHub Enterprise repository on a configured host shows its PR; a host without a source shows "Add a source for <host>".
- Windows: the same flow works from the VM build.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline github` if Rust changed
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- A UI need requires changing `PullRequest` or the command signatures.
- The batch exceeds rate limits on a 100+ repository set.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
