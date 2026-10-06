# Packet 33 — Tags

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium (Git mutation, remote push and delete). |
| Depends on | 11 (Git lifecycle), 16 (ref epochs); UI after Skein step 3. |

## Goal
Create, push and delete tags for one repository or a whole set, so a release can be tagged everywhere in one step.

## Requirements
- **R1 — Create.** Lightweight or annotated (message) tag on the current HEAD or a chosen ref or commit. Signed tags when the user's Git config has a signing key (`tag.gpgSign` or `-s`); never configure signing for the user.
- **R2 — Across a set.** One dialog tags every selected repository with the same name; per-repository target defaults to its current HEAD; preview lists repo → commit before anything runs; per-repository results.
- **R3 — Push.** Push the new tag (`git push origin refs/tags/<name>`), optionally right after creation; never `--tags` wholesale.
- **R4 — Delete.** Delete a local tag; delete the remote tag only with a separate, explicit confirmation naming the remote.
- **R5 — Validation.** Names checked with `git check-ref-format`; existing tag with the same name refused (no force) unless the user explicitly chooses "Move tag", which needs a second confirmation and is local-only until pushed with force-with-lease semantics for tags.
- **R6 — Visibility.** Tags show in the history drawer rails and in the ref picker; ref epochs bump so caches refresh.
- **R7 — Optional GitHub release.** After pushing an annotated tag, offer "Create GitHub release" with notes prefilled from the tag message (draft by default).
- **I1** — Fixed argv, no shell; tag names never start with `-`.

## Done when
On the skein-fixture repositories: tag a set `v2.4.0` annotated, push, see it on GitHub and in the drawer, delete it locally and remotely; a duplicate name is refused; cleanup removes the fixture tags and release.
