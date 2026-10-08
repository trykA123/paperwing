# 48 — Cut the cost and the count of `git status`

Status: ready (evidence below is the owner's diagnostics export `skein-diagnostics-20261008-053859.json`, Windows 11, Git 2.55.0, 16 cores, SSD, 2 repositories)
Platform: Windows first (Defender or EDR on); Linux parity for the behaviour, not the numbers
Size: M
Role: api-builder (backend steps 1-3), then a small ui-builder step (4)

## Goal
Opening a set and returning to the window feel instant. The status of a repository is read at most once per real change, and each read costs less than the ~1.3 s it costs today on the owner's work PC.

## What the diagnostics show
39 minutes of use, two repositories (930 and 1,900 tracked files, one 760 MB pack):

| Finding | Number | Reading |
|---|---|---|
| `git status` calls | 88 (44 per repository), median 1.3 s, max 1.9 s, 114 s in total | The largest single cost in the session. |
| Cost of a trivial Git process | 113-170 ms, even `git version` (117 ms) | A fixed per-process tax from security software; same pattern as the Git 2.55 export of 2026-10-07, so it is not the Git version. |
| `rev-parse` calls | 110, 14.8 s in total | Pure spawn overhead. |
| Real work inside one status | about 1.2 s (1.3 s minus the 0.13 s floor) | Defender scans each file Git stats or reads. Index refresh touches all tracked files and the untracked scan walks the tree. |
| Git peak | 18 processes at once, 1.2 GB, 810 % CPU | A set refresh fans out; with 2 repositories this is already heavy. |
| Skein queue wait | 0 events over 200 ms | Skein's own scheduling is not the bottleneck. |

Why there are so many calls: `src/App.svelte` `onFocus` runs `app.repositories.refreshStatus(true)` on every window focus. `refreshStatus(force)` in `src/lib/state/repositories.svelte.ts` then reads every visible, ticked and chip-judged row again with `app.checkExists`, which calls `local_status` (`src-tauri/src/local.rs`, `status_of`) and spawns `git status --porcelain=v2 --branch` per repository. Switching between Skein and an editor 44 times means 44 full reads of unchanged repositories.

## Already done
- `status_of` in `src-tauri/src/local.rs`: one `git status --porcelain=v2 --branch` per path, one `describe` when detached.
- Packet 17 batch readers (`src-tauri/src/git/batch.rs`) removed spawns from compare, not from status.
- Packet 30 (file watcher, merged 2026-10-08) knows when files change under an open set (`src-tauri/src/watch/`, `src/lib/auto-refresh.ts`). It is the right trigger for status; it is not yet the only one.
- `RepoGit` (`src-tauri/src/git/repo_command.rs`) already disables repository fsmonitor hooks for status. Keep that.

## Decisions
- Status is event-driven, not focus-driven. A window focus never forces a full re-read when the watcher is healthy. If the watcher is not running for a repository (too many roots, error, unsupported root), focus falls back to a read, rate limited to one per repository every 30 s.
- A status result carries a cheap fingerprint (HEAD sha, mtime of `.git/index`, and the watcher's change counter). A re-read is skipped when nothing changed.
- Do not change what the row shows (branch, ahead/behind, dirty count, tag). Only how and when it is computed.
- Measure before choosing the Git flags. The owner times these by hand in the 1,900-file repository and reports the numbers; the packet starts with that:
  `git status --porcelain=v2 --branch` against the same with `-c core.untrackedCache=true`, with `-c core.fsmonitor=true` (built-in daemon on Windows), and with `--untracked-files=no`. Adopt a flag only if it keeps the dirty count correct and cuts the time by at least a third.
- Never write repository config to speed things up. Pass flags per call with `-c`.

## Scope
- Do: step 1 measurement fixture, a status cache keyed by fingerprint, focus and refresh rework, status flag changes that win in measurement, a bounded concurrency for set-wide status, a counter in diagnostics.
- Do not: replace Git with gitoxide here (that is 17b); touch compare, the watcher internals, or the row layout.

## Read first
- `src-tauri/src/local.rs`, `src-tauri/src/git/repo_command.rs`, `src-tauri/src/git/runner.rs`
- `src/App.svelte` (`onFocus`), `src/lib/state/repositories.svelte.ts` (`refreshStatus`, `setVisible`), `src/lib/state.svelte.ts` (`checkExists`, status loader near line 747)
- `src/lib/auto-refresh.ts`, `src-tauri/src/watch/hub.rs`
- `docs/metadata-caching.md`, `plans/packets/17b-gitoxide.md`

## Do not touch
`src-tauri/src/compare*`, `src-tauri/src/watch/` internals, packet 44 shell files other than the focus handler.

## Steps
1. Record the baseline: add `git.status` as its own operation bucket in diagnostics if it is not already, and a count of status calls per trigger (`focus`, `visible`, `watcher`, `manual`). Check: a unit test for the counters; the owner's export shows the new buckets.
2. Remove the forced re-read on window focus when the watcher is healthy for that repository; keep the 30 s rate-limited fallback otherwise. Check: unit test with a fake watcher state: ten focus events cause zero reads when healthy and at most one per 30 s when not.
3. Add the fingerprint skip: a status request returns the cached result when HEAD, index mtime and the watcher counter are unchanged. Check: Rust test with a temp repository: unchanged repository twice spawns Git once; a touched file, a commit and a branch switch each spawn once.
4. Measure the flag candidates on Windows with Defender on (owner's PC) and adopt only the winners. Check: a table of times before and after in the report; the existing status tests still pass, including untracked and renamed entries.
5. Bound set-wide status to 4 concurrent repositories and order visible rows first, so a 50-repository set does not start 50 processes at once. Check: test that the number of running status commands never exceeds 4.
6. Show stale-while-revalidate in the row: the last known status stays visible with no spinner while a re-read runs. Check: screenshot at 1440 px, both themes.

## Done when
- A 39-minute session like the owner's export shows fewer than 10 status calls per repository, not 44.
- Returning to the window after editing in another program shows the correct dirty count within one second, with no visible reload.
- Median status time on the owner's PC drops by at least a third, or the report shows which flag was tried and why it did not help.
- Windows: no Defender prompt, no new idle CPU, and no extra processes while the window is in the background.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory, never `/tmp`)
- `rustfmt --check` and clippy on touched Rust files
- Windows CI green (`test-windows`)

## Stop and report if
- Every flag candidate is within 10 % of today's time: the cost is purely the security scan and the answer is the Defender exclusion in the README plus 17b.
- The fingerprint misses a change the watcher cannot see (network share, junction): report the case; do not widen the fingerprint without a decision.

## Report
Commit sha, files changed, the before/after table from step 4, call counts per trigger before and after, gate results, anything skipped.
