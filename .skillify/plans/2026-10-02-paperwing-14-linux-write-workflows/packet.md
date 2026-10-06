# Packet 14 — Linux edit tickets, saves, directional copies and recovery UI

**Status:** Authorized by the owner; implementation gated on accepted13 and reviewed executable slices. **Weight:** Heavy. **Depends on:** accepted 02,04,09,13.
**Read first:** ticket/copy/Journal façades, comparison `write_context`, `lib.rs`, `api.ts`, FileCompare, CopyOperations, RecoveryPanel and commands.

## Outcome and scope
Enable the same editor/save/copy/recovery workflows on supported Linux roots through fresh authority and the proven backend. No generic filesystem fallback, cached write tickets, new editor architecture or UI redesign.

## Requirements and invariants
- R1: Linux registers edit/open/close/save, copy/preview/apply/cancel and recovery/list/undo/resolve/cleanup with compatible typed contracts.
- R2: Native Linux edit/save/undo, hunk copy and whole-file/folder directional copy work, including creates, overwrites, retained destination-only entries and partial outcomes.
- R3: Dirty close/ref-change/mutation guards, page reload/cancel teardown and per-root capability reasons remain coherent on both platforms.
- I1: Fresh sessions/generations/root/metadata/bytes authorize each write; frozen preview is revalidated before apply and never treated as permission.
- I2: No weakening of Windows TxF or Linux accepted contract; copy batches stay partial and unresolved backup storage is retained.

## Evidence
- [FACT] `lib.rs` currently manages/releases/registers the file service only on Windows; API wrappers call those commands unconditionally.
- [FACT] FileCompare requests tickets before constructing editable working-tree models; content viewing and authorization must stay separable.
- [DECISION] Enable capability only after actual root/backend probe, not `cfg(linux)` alone; unsupported targets remain readable with explained write refusal.

## Steps
- P1 [ISOLATE]: Share only portable ticket/plan orchestration and DTOs; route OS-specific authorization/journal operations to proven backends. Preserve service ownership and façade command names.
  - Depends on: none. Location: extracted files/tickets/copy façades; comparison write-context bridge.
  - Verify: stale generation/root/metadata/source/destination fixtures and exact Windows/Linux IPC serialization tests.
- P2 [ISOLATE]: Register/manage/release Linux services and commands; lifecycle cleanup mirrors Windows without globally removing guards or keeping tickets after page reload.
  - Depends on: P1. Location: `lib.rs::run` registration/page-load release and files command wrappers.
  - Verify: full native Cargo/clippy gates, IPC presence, active session/ticket counts returning to baseline after cancel/close/reload.
- P3 [ISOLATE]: Wire capability-aware native editor actions, hunk copy, preview/apply/recovery and error feedback. Keep Monaco/model/dirty lifecycle in FileCompare and distinguish buffer undo from saved-operation recovery.
  - Depends on: P2. Location: FileCompare, CopyOperations, RecoveryPanel, commands and API.
  - Verify: `bun run --bun check`; `bun test src/lib`; `bun run --bun build`; native Windows/Linux editor/recovery interaction matrix.
- P4 [ISOLATE]: Perform exact-byte/EOL/BOM save and both-direction file/folder copies; cancel mid-batch; external-edit/replaced-root/stale-preview and restart/undo/cleanup drills on supported and unsupported roots.
  - Depends on: P3. Location: marked packet-01 fixtures and accepted 08/13 fault runner.
  - Verify: native owner-observed before/after fingerprints and safe refusal; destination-only retained files and later external edits survive; all old Windows safety tests pass.

## Acceptance
- A1 (static + native fixtures): compatible commands/lifecycle and fresh authority refusal matrix → R1/R3/I1/I2.
- A2 (native owner-observed): complete editor/copy/undo/restart/partial matrix on both platforms → R2/R3/I1/I2.

## Heavy authority and rollback
User approves command rollout and each destructive drill; one writer/dedicated worktree, main session integrates. Test-only profile/roots and verified restore required. Rollback disables new Linux writes but preserves forward recovery reader/export/undo for created records; keep Windows service/records untouched. No cleanup of pending backups. Stop on failed recovery, uncaught stale authority, dirty-buffer loss, unknown-command calls, unsupported-root writes or missing native evidence. This packet—not a viewing-only milestone—supplies required Linux write functionality.

## Revision log
- 2026-10-02: Proposed integration after native primitive/journal acceptance.

- [REV 2026-10-04] Owner approved remaining packets and useful improvements. No repeated rollout permission is needed within that scope.13repair acceptance remains a technical dependency. Planning-only integration decisions, parent-record proposal and ordered slices are retained under `.skillify/evidence/paperwing/14/`; exact storage/schema/authority plans require independent premortem before delegation. No14source or command rollout yet.

- [REV2026-10-04]13 library repairs accepted and integrated. A1 durable diff storage plan independently approved at a832e5f7c699b40c4727fc3e675096535f14f8ecce12b5e0257b46e485474392; isolated worker implementation starts in paperwing-14-a1. Baseline18 comparison tests and two native replay samples preserve exact results/counters. A2 missing-parent creation is planning only. No Linux write command rollout yet.
