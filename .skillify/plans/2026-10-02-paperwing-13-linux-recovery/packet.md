# Packet 13 — Linux durable journal and conflict-safe recovery

**Status:** Accepted Linux library; independent repair review approved and integrated. Application writes remain disabled pending14. **Weight:** Heavy (persistent data). **Depends on:** accepted 12.
**Read first:** accepted Linux contract, extracted Windows Journal/Record and fault tests, guard façade and RecoveryPanel/API DTOs.

## Outcome and scope
Implement Linux persisted before/after backup, state reconciliation, safe undo/resolve/cleanup through proven guards. Still no application save/copy command rollout. Reuse only genuinely portable journal policy; keep incompatible OS identity/durability behind explicit backends.

## Requirements and invariants
- R1: Durable backup/state sequencing survives every accepted interruption boundary and restart without losing recoverable bytes.
- R2: Undo of replace/create refuses changed root/metadata/target; corrupt/torn/incomplete/conflict states and full storage are honest and safe.
- R3: Cross-process journal exclusion, record namespace/version, private permissions and forward recovery/rollback are proven.
- I1: Existing Windows serialized records remain readable/restorable unchanged; never reinterpret Windows volume/index as Linux identity.
- I2: No automatic backup eviction, unresolved-record deletion, unrelated-file cleanup or claim that the whole copy batch is atomic.

## Evidence
- [FACT] Windows Record denies unknown fields and uses root volume/index; simply adding Linux identity to existing records is not a proven compatible migration.
- [FACT] Journal stores verified before/after bytes/checksums and staged state envelopes; tests cover torn state, limits, conflicts, restart and undo-created-file.
- [DECISION] Linux record format/namespace and any common DTO mapping must be ratified in 08; keep test native readers/export available before write rollout.

## Steps
- P1 [ISOLATE]: Implement accepted Linux format/namespace and reader/state reconciliation; add fixtures for foreign/newer/corrupt/torn/legacy Windows records.
  - Depends on: none. Location: proposed `files/journal/linux.rs` or exact 08-approved equivalent; recovery DTO façade.
  - Verify: no Windows fixture bytes/schema change; incompatible/foreign records are retained and refused, not guessed or deleted.
- P2 [ISOLATE]: Implement verified backup/state durability ordering, capacity accounting and cross-process lock using packet-12 primitives.
  - Depends on: P1. Location: Linux Journal open/store/latest/list/used/replace policy.
  - Verify: native second-process contention, full storage, interrupted backup/state/staging/commit transitions and private permissions.
  - Fails if: writing/renaming a state file alone is assumed durable without the contract's required synchronization/proof.
- P3 [ISOLATE]: Implement safe replace/create undo, restart classification, conflict acknowledgement and eligible cleanup. Preserve later external edits and all ambiguous backups.
  - Depends on: P2. Location: Linux journal undo/resolve/cleanup.
  - Verify: fresh/replaced roots, changed-created-file, modified target, corrupt bytes, incomplete state and double-undo fixtures; cleanup never traverses a link or deletes a pending record.
- P4 [ISOLATE]: Run the full fault matrix and a real process-kill/restart recovery drill; prove baseline Windows records still restore and a disabled Linux writer can still read/export/recover new records.
  - Depends on: P3. Location: packet-01 profile/fault runner, Linux/Windows journal suites.
  - Verify: serial native Cargo tests/clippy on both platforms; owner-observed restore with exact bytes/metadata and retained audit manifest. Distinguish process-kill evidence from power-loss evidence.

## Acceptance
- A1 (native fixtures): accepted interruption/restart/durability matrix → R1/I2.
- A2 (native fixtures): conflicts/created-file/root/corruption/full-store/cleanup matrix → R2/I1/I2.
- A3 (static + native owner-observed): lock/privacy/version/foreign-record compatibility and forward recovery rollback drill → R3/I1/I2.

## Heavy authority and rollback
User approves format/backend/dependencies and fixture-only write drills. One writer/dedicated worktree, main session integrates. Protect all existing records and prove restore before mutations. After new records exist, code downgrade alone is unsafe: disable new writes and retain accepted forward reader/export/undo support; restore only manifest-owned fixture data after inspection. Eligible backup cleanup is irreversible and needs explicit confirmation. Stop on failed recovery, namespace collision, unknown-record deletion, unaccepted schema or data drift.

## Revision log
- 2026-10-02: Gated persistent-data implementation, no Linux format selected during planning.

- [REV 2026-10-04] Owner approved all remaining work. Concrete Linux-only journal design and verified guard seams are recorded at `.skillify/evidence/paperwing/13/design-draft.md`.08contract is accepted;12repair recheck is pending after reviewer usage limit. No repeated implementation approval is needed. Persisted record/write rollout remains gated on independent protection/recovery acceptance.

- [REV 2026-10-04] Packet12 independent repair review passed. Proceed with the approved Linux-only namespace, conservative immutable records/states and fixture-only fault/recovery drills; preserve Windows source/records. Exact executable plan: `.skillify/evidence/paperwing/13/implementation-plan.md`.

- [REV 2026-10-04] Native114/119 full suites,15 guard/31 journal tests and41 owned process-kill cases pass. Bounded stack provenance and containment/resource regressions have fail-before/pass-after evidence. All three unsafe containment controls were restored from verified backups. Main finished final verification after the worker/reviewer usage limit and froze18 delta paths. Independent frozen-source approval remains required before integration and14; no Linux write rollout or Windows runtime proof claimed.

- [REV 2026-10-04] Independent frozen review found resumed-unlink lock authority and test-child setup/exit cleanup defects. Repair plan: `.skillify/evidence/paperwing/13/repair-1/plan.md`. Retain the original frozen evidence and add native fail-before/pass-after controls; independent repair approval still gates integration and14.

- [REV2026-10-04] Both review defects have native fail-before/pass-after regressions and independent approval at repair manifest5e66be101c7d821383d74aba68777003ac711d593d4c93dabd320426a63e00ac. Main integrated18 explicit source paths and passed118/123 Rust tests,63 frontend tests/398 assertions, zero Svelte errors/warnings, frontend build, diagnostic Clippy and formatting. Strict Clippy remains101 dead_code, not passed. Normal Linux production build and isolated30-second Wayland GDB smoke pass; all owned PIDs are gone.41 SIGKILL cases are process-interruption evidence; Windows/reboot/power-loss/casefold and owner-observed app workflows remain unverified.14 may proceed through independently reviewed executable slices.
