# Packet 26 — Stash across a set

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium (Git mutation). |
| Depends on | 11 (Git lifecycle); UI after Skein step 3. |

## Goal
Switching a set to another branch works even when some repositories have uncommitted changes: stash, switch, and restore per repository or for the whole set.

## Requirements
- **R1** — Per-repository and bulk "Stash changes", "Apply stash" and "Pop stash", with message and untracked-file option.
- **R2** — "Switch with stash": for each dirty repository, stash, switch, then offer to re-apply. A conflict on apply leaves the stash in place and reports it.
- **R3** — Stash list per repository in the drawer, with diff preview.
- **I1** — Never drop a stash automatically. Partial bulk results are reported per repository.

## Done when
Fixture: a set of three repositories, two dirty, switches branch and restores both stashes; a forced conflict keeps the stash and shows the conflict.
