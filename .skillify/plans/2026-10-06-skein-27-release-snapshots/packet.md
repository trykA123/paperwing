# Packet 27 — Release snapshots

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium. |
| Depends on | 16 (metadata keys); compare UI from Skein step 6. |

## Goal
Save the exact commit of every repository in a set as a named snapshot, then restore it or compare the set against it.

## Requirements
- **R1** — "Save snapshot" records per repository: remote URL, branch, full commit ID, dirty flag. Stored in settings as data, never as authority.
- **R2** — "Restore snapshot" checks out each commit (detached or on a new branch, user's choice), refusing dirty repositories unless stashed (26).
- **R3** — "Compare with snapshot" opens set compare with the snapshot as the left side.
- **R4** — Export and import a snapshot as JSON, for sharing a release manifest.
- **I1** — Restoring never fetches silently; missing commits are reported with a fetch action.

## Done when
On the fixture repositories: save, move branches, compare shows the difference, restore returns every repository to the saved commit.
