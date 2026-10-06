# Packet 29 — Discard changes and partial staging

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium (destroys working-tree changes). |
| Depends on | 14 (Linux recovery records). |

## Goal
Revert a file or a single change, and stage individual changes within a file in the commit dialog.

## Requirements
- **R1** — Discard file and discard hunk from the commit dialog and the file view. Each discard writes a recovery record first, so it can be undone.
- **R2** — Stage and unstage individual hunks (and line ranges) through `git apply --cached`.
- **I1** — Discard needs a confirmation naming the files. No discard without a recovery record.

## Done when
Fixture: stage one of two hunks, commit, the other stays; discard a hunk and undo it from Recovery.
