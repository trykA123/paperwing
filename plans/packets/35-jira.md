# 35 — Jira (read-only) as an IssueProvider, and branch from ticket

Status: blocked by 38 (core boundaries) and 34 (store); UI layout is decided
Platform: Windows first, Linux parity
Size: L
Role: api-builder (provider, client), then ui-builder

## Goal
The user sees Jira boards, sprints and issues inside Skein, links tickets to branches and pull requests, and creates a branch for a ticket across repositories. Jira is read-only.

## Already done
- No Jira code. No `IssueProvider` yet: packet 38 defines it (`plans/packets/38-core-boundaries.md`) with the `IssueUpdated` event, a registry that builds a provider only when enabled, and the enabled flag in the 34 store.
- The layout is chosen. Commit `bdb3a81` on the integration branch records it: a kanban board in the main area, boards, sprints and saved filters in the rail panel, and the ticket as a centred floating dialog. Prototype: https://claude.ai/artifact/5aK3CYYU1YeYid9SxWbtQC.
- Reusable: branch creation (`create_branch` in `src-tauri/src/commit.rs`, `src/components/BranchDialog.svelte`), ref-name validation (`valid_ref`), PR lookup (28), keyring storage in `src-tauri/src/credentials.rs`.

## Decisions
- Build against `IssueProvider` from 38. The UI uses provider-neutral types (`Issue`, `Board`, `Sprint`, `IssueLink`), so another tracker can replace Jira.
- Read-only: Jira Cloud REST v3 plus the Agile API, or Data Center with a personal access token. The client exposes GET only. A test proves no write method exists.
- Layout "board with centred ticket dialog": the board is the main view; the dialog has two columns (description, sub-tasks and links on the left; properties, branches and PRs, and "Create branch…" on the right). Branch-from-ticket opens as a modal from that dialog. No right drawer.
- Credentials: site URL, email and API token (Cloud) or PAT (Data Center) in the OS keyring under the existing service name. The company may run an internal Jira host: the site URL is a setting, never hardcoded. Connection test before saving.
- Name template `{type}/{key}-{slug}`, editable, validated with `valid_ref`. The commit dialog prefills `KEY: ` when the branch carries a key.
- Refresh on open and on demand with conditional requests where supported, a shared rate limiter, data cached in the 34 store. A disabled provider does nothing: no worker, no traffic, no cache rows.

## Scope
- Do: Jira client and provider, credentials and connection test, boards, sprints, filters, JQL box, issue dialog, issue-key detection in branch names and commit subjects, branch from ticket, commit prefix.
- Do not: transitions, edits, comments, assignment; webhooks; other trackers.

## Read first
- `plans/packets/38-core-boundaries.md`, `docs/architecture.md` (after 38)
- `src/components/BranchDialog.svelte`, `src/components/CommitDialog.svelte`, `src/components/panel/ComparePanel.svelte` (rail panel pattern), `src/lib/rail.ts`
- `src-tauri/src/credentials.rs`, `src-tauri/src/commit.rs` (`create_branch`)
- The 10-question checklist in `plans/packets/TEMPLATE.md`; answer it in the commit message

## Steps
1. Provider-neutral issue types and the Jira client with a test seam, recorded Cloud and Data Center responses, GET-only enforcement. Check: tests for pagination, rate limits, auth failure and the GET-only guarantee.
2. Credentials and connection test, with the provider enable toggle. Check: disabled-provider test shows zero requests and zero cache rows.
3. Rail section "Tickets" with boards, sprints and saved filters; board view with columns by status; filters (mine, sprint, epic, type, text) and the JQL box. Check: screenshots at 1440 and 1100 px, both themes.
4. Ticket dialog (two columns), rendered description, sub-tasks, links, epic. Check: browser test with long descriptions.
5. Linking: key detection in branch names and commit subjects, ticket chip on table rows, linked branches and PRs on the ticket (PR data from 28). Check: unit tests for key patterns.
6. Branch from ticket: choose repositories and base branch each, template, optional switch and push with upstream, preview, per-repository results. Commit prefix. Check: fixture with two repositories and one expected failure.

## Done when
- Against a Jira Cloud test site, or recorded responses if none is available: boards and sprint render, a ticket shows its linked branches, and branch-from-ticket creates the branch in two fixture repositories with one expected failure reported.
- The UI says "Read-only" and links "Open in Jira".
- Windows: the same flow runs on the VM build; the token is read from Credential Manager.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- `IssueProvider` from 38 cannot express boards and sprints.
- A Jira endpoint needed for the board is not available with a read-only token.

## Report
Commit sha, files changed, each step's check result, the answered checklist, gate results, anything skipped.
