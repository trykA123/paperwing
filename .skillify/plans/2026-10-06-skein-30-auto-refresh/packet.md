# Packet 30 — Automatic refresh

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium. |
| Depends on | 16 (invalidation); 19 (foreground priority). |

## Goal
Local status updates by itself when files change, and an optional timed fetch keeps "behind" counts current.

## Requirements
- **R1** — Watch open set roots with the `notify` crate (pinned); debounce 300 ms; ignore `.git/objects` churn and build folders; refresh only affected repositories.
- **R2** — Optional background fetch every N minutes (off by default), lowest priority, paused on battery saver or metered network where detectable, never during a foreground Git command.
- **I1** — Watchers are released when a set closes; a watch error falls back to manual refresh with a notice.

## Done when
Editing a file in a fixture repository updates its row within a second without manual refresh; watcher count returns to zero after closing the set.
