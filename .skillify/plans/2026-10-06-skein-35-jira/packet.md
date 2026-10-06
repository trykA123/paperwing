# Packet 35 — Jira (read-only) and branch from ticket

| | |
|---|---|
| Status | Approved 2026-10-06; layout direction chosen from the Skein Tickets prototype. |
| Weight | Medium (new external API, credentials; Git writes only for branch creation). |
| Depends on | 34 (store), Skein step 3b (rail), 33 (shared naming validation). |

## Goal
See Jira boards, sprints and issues inside Skein without leaving it, link tickets to branches and PRs, and create a branch for a ticket across repositories.

## Requirements
- **R1 — Read-only.** Jira Cloud REST v3 and Agile API, or Data Center with a personal access token. Only GET requests; no transitions, edits, comments or assignment. The UI says "Read-only" and links "Open in Jira".
- **R2 — Views.** Boards (scrum, kanban), active sprint, columns by status, issue detail (rendered description, subtasks, links, epic), filters (mine, sprint, epic, type, text) and a JQL box.
- **R3 — Linking.** Detect issue keys in branch names and commit subjects; show linked branches and PRs per repository on the ticket, and the ticket chip on table rows.
- **R4 — Branch from ticket.** Choose repositories and base branch per repository, name template `{type}/{key}-{slug}` (editable, validated with `git check-ref-format`), optional switch and push with upstream, preview, per-repository results.
- **R5 — Commit prefix.** Commit dialog prefills `KEY: ` when the branch carries a key.
- **R6 — Credentials.** Site URL and email plus API token (Cloud) or PAT (Data Center) in the OS keyring; scopes read-only; connection test.
- **R7 — Refresh.** On open and on demand, conditional requests where supported, shared rate limiter from 34; data cached in the store.
- **I1** — No Jira write request exists in the code (enforced by a test that the client exposes GET only).

## Done when
Against a Jira Cloud test site (or recorded responses if none is available): boards and sprint render, a ticket shows its linked branches, and branch-from-ticket creates the branch in two fixture repositories with one expected failure reported.

- [REV 2026-10-06] Owner chose direction A from https://claude.ai/artifact/5aK3CYYU1YeYid9SxWbtQC: kanban board in the main area, boards/sprints/saved filters in the rail panel, and the ticket detail as a centred floating dialog (two columns: description, sub-tasks and links left; properties, branches/PRs and "Create branch…" right), not a right drawer. Branch-from-ticket opens as a modal from that dialog.
