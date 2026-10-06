# Packet 31 — GitHub Actions view

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium (GitHub API, credentials, write actions on workflows). |
| Depends on | 16 (source scope, credential revision), 28 (per-row check status, shared GitHub client). |

## Goal
See and control GitHub Actions for a repository and for a whole set: runs, jobs, steps, logs, re-run, cancel and manual dispatch.

## Requirements
- **R1 — Set board.** For each repository in a set: the latest run of each workflow on the current branch, with status, duration, trigger and commit. Filter by status and branch.
- **R2 — Repository view.** Workflow list and paginated run history; a run opens jobs and steps with durations and annotations.
- **R3 — Logs.** Job logs in a virtualised viewer with step folding, search and ANSI colour; download the run's log archive. Logs appear when the job's API returns them; no claim of live streaming.
- **R4 — Actions.** Re-run all, re-run failed jobs, cancel a run, and dispatch a `workflow_dispatch` workflow with its declared inputs (parsed from the workflow file at the chosen ref). Each action confirms first and reports the result.
- **R5 — Artifacts.** List and download run artifacts to a user-chosen folder.
- **R6 — Updates.** Poll only while the view is open: every 10 s for in-progress runs, 60 s otherwise, using conditional requests (ETag) to protect the rate limit. Back off on rate-limit responses and show the reset time.
- **I1** — Uses the stored token only. Missing permissions (Actions read/write, `workflow` scope) produce a clear message naming the needed permission.
- **I2** — No background polling when the view is closed; no secrets or log content persisted beyond the download the user asked for.

## Steps
1. GitHub client additions in Rust with a test seam: runs, jobs, logs (redirect handling), artifacts, re-run, cancel, dispatch. Check: recorded-response tests including pagination, 304, 403 rate limit and permission errors.
2. Set board and repository view UI in the Formation style, with the Rails loop for running states. Check: browser screenshots in both themes.
3. Log viewer and actions with confirmations. Check: browser tests with large logs (100k lines).
4. Native acceptance on a fixture repository with a small workflow (push, dispatch with inputs, failing job, re-run failed, cancel, artifact download), then cleanup.

## Done when
On skein-fixture-api with a fixture workflow: the board shows the run, logs open, re-run failed and dispatch work from Skein, and an artifact downloads; polling stops when the view closes.
