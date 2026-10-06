# Packet 15 — Linux clone, reclone and desktop trash

| | |
|---|---|
| Status | Proposed. Owner approved the remaining work on 2026-10-04 (see revision log). Each destructive action still needs its own confirmation. |
| Weight | Heavy (moves and recycles folders). |
| Depends on | Accepted 09 (platform paths), 11 (Git lifecycle), 12 (Linux file guards). |
| Primary platform | Windows (work use). This packet adds Linux parity and must leave Windows behaviour unchanged. |
| Read first | `clone.rs`, `trash.rs`, registered clone destinations and set roots, state clone/delete handlers, SetView removal dialog, accepted Linux guard contract. |

## Goal
On Linux, clone, open, fetch, pull, switch, reclone and "remove set" (move folders to the trash) work like they do on Windows, with the same protections. Folder operations stay separate from file-recovery transactions.

## Why
- The clone/reclone protections that use native handles exist only on Windows; on Linux the code does a plain rename.
- On Linux, recycling always refuses today.
- `deleteSet` can warn that recycling failed and still delete the set's configuration. Changing that is a product decision, not a refactor.

## Scope
- **In:** Linux clone/reclone destination checks and preservation; desktop-trash recycling of registered folders; honest partial-failure reporting.
- **Out:** permanent delete as a fallback, pretending a batch is atomic, any change to Windows Recycle Bin behaviour.

## Requirements
- **R1** — Clone and reclone destinations, and preservation of an existing checkout, use the accepted Linux identity, confinement, origin/ref and clone-lease checks.
- **R2** — Confirmed removal moves only registered, unshared, eligible folders into the desktop trash, and reports partial failures honestly.
- **R3** — When recycling fails, the configuration rules below apply. Unsafe, shared, unregistered or changed targets are left in place.
- **I1** — Never delete preserved reclone data automatically, follow swapped parent folders, merge case-distinct roots, or touch a real repository during drills.
- **I2** — Windows protections, Recycle Bin behaviour, explicit Git mutation and separate push stay exactly as they are, unless a separately accepted error-flow fix is documented.

## Decisions
- **D1 — Recycle failure keeps the set.** If recycling fails, the set stays configured until the user explicitly chooses "remove configuration only". The app never claims folders were recycled when they were not.
- **D2 — Use desktop-trash semantics** (freedesktop trash via GIO or an equivalent pinned backend), never `remove_dir_all`. Research and measurements: `.skillify/evidence/paperwing/15/desktop-trash-research.md`.
- **D3** — Ordinary app folder actions keep their existing explicit confirmation flow.

## Steps
1. **Ratify the backend and error flow.** Record behaviour for cross-device moves, shared roots, replaced roots and restore paths.
   - Check: the owner accepts the destructive and error-flow policy; pinned dependency/API review; native capability probe; a restore drill on a sacrificial folder.
2. **Apply the Linux guards to clone and reclone:** destination, preservation, registered root and origin identity. Preserved folders get unique paths and are never cleaned up implicitly.
   - Where: `clone.rs::{validate, run_job, start_clone}`, registration, guard backend.
   - Check: marked local clones; moved root, metadata or origin; a collision with an existing preserved folder; cancel and failure; case-distinct destinations.
3. **Implement desktop recycling** with identity, registration and sharing guards. Update the confirmation, partial-error and "configuration only" UI exactly as ratified.
   - Where: `trash.rs::{trash_folders, recycle}`, AppState `deleteSet`, SetView removal dialog.
   - Check: trash and restore a sacrificial checkout natively; shared, unregistered, changed and cross-device failures keep data and configuration as specified; backend and UI agree.
4. **Run the full matrix** on both platforms (all clone modes, partial, cancel, reclone, recycle) with before/after root manifests.
   - Check: frontend, Cargo and Clippy gates; owner-observed preservation and restore; Windows safeguards unchanged.

## Done when
- **A1 (native fixtures):** the clone, origin, destination, identity, cancel and preservation matrix passes. Covers R1, I1, I2.
- **A2 (native, owner-observed):** eligible trash and restore, protected folders, partial failures and configuration-failure cases pass. Covers R2, R3, I1, I2.

## Authority and rollback
The owner decides the backend, policy and dependencies, and approves each folder move or cleanup. One writer works in a dedicated worktree; the main session integrates. Prove restore from both a preserved folder and the desktop trash before any mutation drill. Rollback disables the new folder actions and restores only manifest-owned data after checking identity; preserved and trashed folders stay reachable. Permanent and remote deletion are not authorized.

Stop on a failed restore, a target outside the root, an ambiguous shared root, silently removed configuration or data, or an unaccepted Windows error-flow change.

## Revision log
- 2026-10-02: Proposed destructive-folder packet; backend choice and failure semantics are explicit approval gates.
- [REV 2026-10-04] Owner approved all remaining work, improvements and isolated drills; no repeat generic approval needed. Desktop-trash preflight and GIO measurements recorded at `.skillify/evidence/paperwing/15/desktop-trash-research.md`. Backend authority, native restore and the exact failure flow remain technical acceptance gates. Failed requested recycles keep configuration until explicit configuration-only deletion. No packet 15 source change or live trash mutation has run.
- 2026-10-05: Rewritten in plain format; Windows marked primary and unchanged by this packet. No change in scope.
