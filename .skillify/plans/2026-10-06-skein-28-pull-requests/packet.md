# Packet 28 — Pull requests and CI status

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium (GitHub API, credentials). |
| Depends on | 16 (source scope, credential revision). |

## Goal
See the pull request and check status of each repository's current branch in the table, and open a pull request from a branch.

## Requirements
- **R1** — Per row: PR number, state (draft, open, merged), review state and combined check status, fetched for the current branch only, on demand and on refresh.
- **R2** — "Open pull request" with title, body, base branch and draft flag; opens the created PR in the browser.
- **R3** — Bulk "Open pull requests" for a set branch across repositories, with per-repository results.
- **I1** — Uses the stored token only; no new credential storage. Rate limits are reported, not retried in a loop.

## Done when
On skein-fixture-api: push feature/login, open a draft PR from Skein, the row shows the PR and its state; cleanup closes the PR.
