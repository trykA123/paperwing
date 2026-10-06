# Packet 09 — native roots, path policy, identity and capabilities

**Status:** Implemented on Linux; independent review approved. Native Windows acceptance remains unavailable. **Weight:** Heavy (persisted/public/native boundary). **Depends on:** accepted 02,03 and packet-08 contract.
**Read first:** `workspace.ts`, extracted workspace paths/registration, `paths.rs`, Git validation, `clone.rs`, `trash.rs`, `lib.rs`, `api.ts`, commands and FileCompare/FolderCompare.

## Outcome and scope
Windows/Linux agree on native destinations and identity; UI/backend describe root-specific support explicitly. No Linux file writes enabled here. Preserve saved roots/settings rather than guessing translations. New command/types/modules below are proposals.

## Requirements and invariants
- R1: New workspaces require a valid native root; existing Windows data loads unchanged on Windows and foreign roots prompt for explicit owner reassignment.
- R2: Confinement/metadata protection are separated from Windows aliases and native filename policy; frontend/backend destinations agree.
- R3: Row/logical path/collision/physical identity are distinct; case-distinct Linux folders are not conflated by unconditional lowercasing.
- R4: Typed capability and root-probe IPC informs buttons, palette/shortcuts, previews and direct calls; unavailable operation returns a reason.
- I1: Never relax traversal, protected metadata, linked ancestors or write authorization. Capability flags are not authorization tokens.
- I2: Preserve legacy settings/Windows layout vectors and unsupported-content/UTF-8 limits unless an explicit contract revision is accepted.

## Evidence
- [FACT] `defaultWorkspace()` uses `C:\\Dev\\repos`; `dest` infers separator from text; binding uses native `PathBuf::join`.
- [FACT] clash/clone/trash/root-sharing logic lowercases path strings; Linux filesystems cannot be assumed universally case-sensitive or insensitive.
- [DECISION] Ask for native root selection rather than silently converting a persisted `C:` path. Keep platform-dependent output covered by tagged shared vectors.

## Steps
- P1 [ISOLATE]: Specify/add proposed `platform_info` and root-support probe DTO/commands with platform/path/case/capability reasons; conform to packet 08. Keep write support false until backend proof is integrated.
  - Depends on: none. Location: proposed `src-tauri/src/platform.rs`, `lib.rs`, `api.ts`, initialization state.
  - Verify: exact IPC payload/serialization fixtures and native valid/invalid/unsupported-root probes.
- P2 [ISOLATE]: Make root initialization explicit/native; preserve old settings and reject foreign roots without mutating them. Normalize template grammar separately from native filename spelling.
  - Depends on: P1. Location: `workspace.ts::defaultWorkspace`/migration, `workspace-paths.ts`, backend registration, root picker/settings.
  - Verify: `bun test src/lib`; shared Rust layout fixtures for Windows/Linux roots, trailing separators, spaces/Unicode and literal native characters.
  - Trap: hardcoding `/home/claud`, interpreting every Linux backslash as a separator, or silently rewriting all saved sets.
- P3 [ISOLATE]: Split path confinement from Windows alias/device rules; implement accepted native policy and authoritative physical identity comparisons. Update every clash/destination/dedup/sharing reader, not only `dest`.
  - Depends on: P2. Location: `paths.rs::relative/ReadRoot`, Git `valid_path/valid_root`, registration/set roots, clone/trash validation and frontend identity helpers.
  - Verify: native path attack/metadata tests; case-distinct and case-folded-volume fixtures where supported; replacement of root/metadata; exact destination parity.
  - Fails if: string lowercasing is treated as filesystem identity or widening valid names bypasses metadata protection.
- P4 [ISOLATE]: Route action eligibility/reasons through capabilities and per-root probes. Reading comparison bytes must not depend on ticket availability; unsupported writes stay unavailable without hiding legitimate read-only diffs.
  - Depends on: P3. Location: FileCompare content/ticket initialization, FolderCompare/CopyOperations, commands/editor/copy registry, recovery and set-removal controls.
  - Verify: native HEAD↔working diff renders during intermediate unsupported-write state; UI/shortcut/direct-request refusal agrees; Windows supported-root editing still acquires fresh tickets.

## Acceptance
- A1 (fixture + native): new/existing/foreign roots and tagged destination vectors → R1/R2/I2.
- A2 (native adversarial fixtures): case/physical identity, root/metadata/link/traversal refusal → R2/R3/I1.
- A3 (fixture + native owner-observed): typed capabilities/actions/read content behave consistently; backend authorizes independently → R4/I1/I2.

## Commands and Heavy rollback
Run common frontend/build/Cargo/clippy/diff gates on Windows and Linux; native path/probe fixtures from 01/08. User owns settings migration/native policy. Dedicated worktree, one writer. Back up and restore a sacrificial legacy settings file before changes; rollback restores owned code and original test settings, not recovery records. Stop on unaccepted persisted/IPC change, data drift, broken Windows vectors or weaker containment. Intermediate read-only support does not satisfy final Linux parity.

## Revision log
- 2026-10-02: Proposed portability contract; all new IPC names are design proposals.

- [REV 2026-10-03] Owner accepted the packet08 practical contract. Probes describe native existing roots and physical identity; unknown case policy stays explicit. Missing paths have no physical identity. Linux direct write requests return explicit unsupported reasons; reads do not acquire unavailable tickets.

- [REV 2026-10-03] P1–P4 implemented and independently approved for the Linux read-only boundary. Frontend57/372 and defaultRust43/feature48serial pass; eleven390/1440browser cases and rebuilt native read-only Monaco/direct-refusal proof pass. Two native baseline samples retain exact result/counters. Normal production build excludes instrumentation; CSS unchanged. Missing destinations bind only by exact native paths, never folded candidates. Windows-native/casefold/power-loss checks remain unavailable; strictClippy remains failed as documented. Linux writes remain gated. See docs/implementation-status.md and ignored09 evidence.
