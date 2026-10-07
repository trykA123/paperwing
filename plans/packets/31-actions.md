# 31 — CI runs and logs, GitHub Actions first

Status: blocked by 38 (core boundaries) and 34 (store); needs 28's GitHub client patterns
Platform: Windows first, Linux parity
Size: L
Role: api-builder first (provider and client), then ui-builder

## Goal
The user sees and controls CI for a repository and for a whole set: runs, jobs, steps, logs, re-run, cancel and manual dispatch. GitHub Actions comes first. The same views must work later for Jenkins and other systems through the provider trait.

## Already done
- Nothing for CI. There is no `CiProvider` yet: packet 38 defines it (`plans/packets/38-core-boundaries.md`) with `CiStarted` and `CiCompleted` events and a registry that constructs a provider only when enabled.
- GitHub plumbing to reuse: `src-tauri/src/github/http.rs` and `http/` (host-aware client, rate-limit error; no ETag or conditional-request support yet, step 1 adds it), `src-tauri/src/github/pulls/source.rs` (resolves the source from the repository remote, including GitHub Enterprise), `PullChecks` in the PR model.

## Decisions
- Build against `CiProvider` from 38. The UI talks to provider-neutral types (`CiRun`, `CiJob`, `CiStep`, `CiLog`, `CiArtifact`), never to GitHub JSON. A `provider` and `host` field on every item.
- The GitHub implementation lives in `src-tauri/src/github/actions/` and resolves the host per repository (github.com, any Enterprise host). Never hardcode github.com.
- Jenkins comes later as a second `CiProvider`. This packet must not leak GitHub concepts (workflow files, `workflow` scope) into the neutral types except as optional capability flags (`canDispatch`, `canRerunFailed`).
- Updates: poll only while the view is open. 10 s for in-progress runs, 60 s otherwise, conditional requests (ETag), back off on rate limits and show the reset time. No background polling with the view closed.
- Logs show when the API returns them. No claim of live streaming. Log content is never persisted beyond a download the user asked for.
- Write actions (re-run all, re-run failed, cancel, dispatch) confirm first and report the result. Dispatch inputs are parsed from the workflow file at the chosen ref.
- Missing permissions produce a message naming the permission needed.
- Set board on orgs with 100+ repositories: load only visible rows and selected rows, concurrency 4.
- Run page matches GitHub's (owner request 2026-10-07; shell direction A, prototype `.alt/shell-design-2/a-final.html`). Module sidebar lists Summary, jobs (status, duration) and Run details. Summary shows trigger, status, duration, job graph, annotations and artifacts. Job view shows steps that expand to their logs (line numbers, `##[group]` folding, ANSI, error lines, timestamps toggle).
- Job graph comes from `needs` in the workflow file at the run's `head_sha`; the REST jobs list has no `needs`. Matrix jobs group by name prefix.
- Annotations come from the job's check run (`check-runs/{id}/annotations`).
- In-progress jobs show step status only; the log loads when the job finishes. Step summaries (`$GITHUB_STEP_SUMMARY`) have no public API: show "Open on GitHub" instead.
- Actions and Pull requests scope to the active set by default, with a chip to widen to all repositories.

## Scope
- Do: provider-neutral CI types and commands, GitHub Actions provider, set board, repository view, log viewer, actions, artifacts download.
- Do not: build Jenkins; store logs; change the PR column (28).

## Read first
- `plans/packets/38-core-boundaries.md`, `docs/architecture.md` (after 38)
- `src-tauri/src/github/http/`, `src-tauri/src/github/pulls/source.rs`, `src-tauri/src/github/pulls/client.rs`
- `src/components/VirtualList.svelte`, `src/components/set/FormationRow.svelte`, `src/lib/api.ts`
- The 10-question checklist in `plans/packets/TEMPLATE.md`; answer it in the commit message

## Steps
1. Provider-neutral types and trait usage; GitHub Actions client with a test seam: runs, jobs, logs (redirect handling), artifacts, re-run, cancel, dispatch. Check: recorded-response tests for pagination, 304, 403 rate limit, permission errors and an Enterprise host.
2. Disable path: a disabled provider is never constructed and makes no requests. Check: counting-transport test.
3. Set board and repository view in the Formation style: latest run per workflow on the current branch, status, duration, trigger, commit; filters. Check: screenshots, both themes.
4. Run page and log viewer: summary, job graph, annotations, artifacts; job view with steps folding into their logs; virtualised, search, ANSI colour, download archive. Check: browser test with a 100k-line log, a failed step, an in-progress job and a matrix run; screenshots against the prototype.
5. Actions and artifacts with confirmations; rail badge for failed runs through the badge map from 03b. Check: browser tests with a mocked provider.
6. Native acceptance on `skein-fixture-api` with a small workflow, then cleanup.

## Done when
- On `skein-fixture-api` with a fixture workflow: the board shows the run, logs open, re-run failed and dispatch work, an artifact downloads, and polling stops when the view closes.
- A GitHub Enterprise repository on a configured host works; a host without a source says so.
- Windows: the same acceptance runs on the VM build.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- `CiProvider` from 38 cannot express runs, jobs, logs or actions without GitHub-specific fields.
- The neutral types need a change that would break 35's `IssueProvider`.

## Report
Commit sha, files changed, each step's check result, the answered checklist, gate results, anything skipped.
