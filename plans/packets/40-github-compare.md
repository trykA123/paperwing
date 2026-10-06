# 40 — Compare on GitHub without cloning

Status: ready (order after the speed work: owner to confirm)
Platform: both; Windows first (work PC, 800-repo GitHub Enterprise org)
Size: L
Role: both (backend first: api-builder; then compare UI: ui-builder)

## Goal
The owner compares two branches, tags or commits of a repo that is not cloned. Skein asks the GitHub compare API on github.com or any GitHub Enterprise host. It shows the file list, line counts and read-only diffs. Today such a compare fails with `notCloned`.

## Already done
- `src-tauri/src/github.rs`: `api_base(host)` maps `github.com` to `https://api.github.com` and every other host to `https://<host>/api/v3`. `valid_name`, `enc`, `get_commits` (pattern for an authenticated GET with `credentials::metadata_revision`).
- `src-tauri/src/github/http.rs`: `Http::connect_at(source, revision)`, `get`, pagination, error mapping.
- `src-tauri/src/github/commit_cache.rs`: store-backed cache example.
- `src-tauri/src/compare.rs`: `read_root` returns `UnavailableReason::NotCloned`. `CompareSnapshot`, `CompareFile`, `CompareLines`, `CompareStatus` are the result shapes (`src/lib/api.ts`).
- `src-tauri/src/store/`: SQLite store (packet 34).

## Decisions
- Use only for one repo: both endpoints are refs of the same remote repo (`branch`, `remoteBranch`, `tag`, `commit`). Two different repos, `head` and `workingTree` stay local only.
- Endpoint: `GET /repos/{owner}/{repo}/compare/{base}...{head}` (three dots, merge-base diff), with `per_page=100` and `page` for commits. The host always comes from the source through `api_base`. Never hardcode github.com.
- Limits: the API returns at most 300 files and 250 commits per compare. If either limit is hit, the snapshot carries `truncated: { files: bool, commits: bool }` and the UI shows an inline warning with the action "Clone to see everything" (uses the existing clone flow).
- Local first: when the repo is cloned and both refs exist locally, keep the local compare. GitHub is used when the item is not cloned, or a ref is missing locally and the user picks "Compare on GitHub".
- File content for the diff view: blobs by sha through `GET /repos/{owner}/{repo}/git/blobs/{sha}` with `Accept: application/vnd.github.raw`. Files over 5 MB or binary: metadata only, no content. Use the `patch` field only for line counts when present; compute lines from blobs when the user opens a file.
- Read-only: no editing, saving or copying to a GitHub side.
- Cache: in the store, keyed by host, owner, repo, base sha and head sha (immutable once both shas are resolved). Blobs are cached by sha with an LRU cap of 200 MB in the app cache directory, not in SQLite.
- Rate limits: read `X-RateLimit-Remaining` and `Retry-After`. At 0 or on a secondary limit, stop and show "GitHub rate limit, try again at HH:MM" (owner's local time). Never retry in a loop.
- Errors map to existing `CompareProblem` kinds plus `githubUnavailable`, `githubNotFound`, `githubRateLimited`. Messages never include tokens. URLs may include host/owner/repo.

## Scope
- Do: `src-tauri/src/github/compare.rs` (request, pagination, mapping to compare rows), blob fetch plus cache, a compare source switch in `compare.rs` where `NotCloned` is returned today, new problem kinds, TS types, compare UI warning and "Compare on GitHub" action.
- Do not: compare across forks or unrelated repos; support GitLab or other providers (packet 38 adds the provider boundary); write to GitHub.

## Read first
1. `src-tauri/src/github.rs`, `github/http.rs`, `github/commit_cache.rs`
2. `src-tauri/src/compare.rs` (`read_root`, `Problem`, snapshot building), `compare/registration.rs`
3. `src/lib/api.ts` (compare types), `src/lib/compare-state.svelte.ts`, `src/components/SetCompare.svelte`, `FolderCompare.svelte`
4. GitHub REST docs: "Compare two commits", "Get a blob" (check the GHES version notes for both)
5. `~/.agents/rules/code-quality.md`, `rust.md`, `typescript.md`

## Do not touch
- Whatever packet 17 (`crew/api-builder-sfzxd`) rewrites in `compare/` and `git/` until it is merged. Rebase onto main after it lands.

## Steps
1. `github/compare.rs`: fetch and map one compare with pagination and limit detection. Check: unit tests against a local HTTP fixture (the test-profile GitHub endpoint) for github.com and a GHES host, 300-file truncation, 250-commit truncation, 404, rate limit.
2. Blob fetch plus LRU cache. Check: test hits the fixture once for two reads of the same sha; cap eviction test.
3. Compare source switch: `NotCloned` plus same-repo refs → GitHub path; snapshot with `source: 'github'` and `truncated`. Check: compare test with a not-cloned item returns a ready snapshot from the fixture.
4. Store cache by resolved shas. Check: second compare does no HTTP request.
5. ui-builder: "On GitHub" badge in the compare header, truncation warning with "Clone to see everything", "Compare on GitHub" action when a ref is missing locally, read-only file view. Check: screenshots at 1440 and 1100 px, both themes.

## Done when
- A not-cloned repo on a GHES host and on github.com compares branch to branch, and the file view shows a read-only diff.
- Over 300 files shows the warning and the clone action.
- Rate limit shows the retry time and makes no further requests.
- Windows: verified on the VM against github.com; GHES verified by the owner at work.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (`--write` for CSS changes)
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory, never `/tmp`)
- `rustfmt --check` and clippy on touched Rust files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- The GHES versions in use lack the compare pagination or raw blob media type.
- The test-profile GitHub endpoint cannot serve the fixtures without changing `test_profile.rs` beyond adding routes.
- The switch in `compare.rs` conflicts with packet 17's merged code in a way that needs a design change.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
