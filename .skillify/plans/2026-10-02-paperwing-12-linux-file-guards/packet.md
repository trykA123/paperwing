# Packet 12 — Linux protected filesystem primitives

**Status:** Implemented and tested; independent repair recheck approved. **Weight:** Heavy (filesystem protections). **Depends on:** accepted08/09/11.
**Read first:** accepted `docs/linux-write-contract.md`, Windows guard façade, `paths.rs`, registered write-root checks and 08 attack evidence.

## Outcome and scope
Implement the accepted Linux guard/identity/replacement/removal/private-storage primitives as a narrow OS backend. No journal, editor/copy command enablement or relaxing Windows protections. Proposed location: `src-tauri/src/file_guard/linux.rs` and shared façade/types **as amended by 08**.

## Requirements and invariants
- R1: Linux root/ancestor/metadata/leaf identity and confinement support the exact accepted contract under adversarial changes.
- R2: Existing/missing replacement, safe created-file removal, hardlink isolation, permission/metadata and unsupported filesystem behavior match that contract.
- R3: Private temporary/recovery storage and cross-process primitive lifecycle have bounded ownership and cleanup.
- I1: No pathname check followed by unsafe open/rename/unlink gap may be misrepresented as guarded authority; fresh checks cannot be bypassed by UI capabilities.
- I2: Windows TxF/share-mode/identity API remains unchanged; no global cache retains native handles.

## Evidence
- [FACT] Current `file_guard.rs` is Windows-only; current Linux `ReadRoot` relies on pathname metadata/open checks.
- [DECISION] Device/inode/mount/descriptor policy and actual syscall/binding choices come from accepted 08, not assumptions in this packet.

## Steps
- P1 [ISOLATE]: Confirm/amend this packet with exact accepted signatures, guarantees and support-probe tests from 08. Reproduce its sacrificial positive controls and restore drill before writing production primitives.
  - Depends on: none. Location: Linux contract/guard façade and test manifest.
  - Verify: contract approval reference and reproducible native fixtures; stop if any mechanism is still unspecified.
- P2 [ISOLATE]: Implement root/ancestor/metadata acquisition and confined reads/parent creation through the accepted native ownership model; add per-root support probes and held-resource limits.
  - Depends on: P1. Location: Linux guard module, `paths.rs::ReadRoot`, platform probes.
  - Verify: native ancestor/root/metadata substitution, linked paths, traversal and unsupported-volume fixtures, including unprotected attack controls.
- P3 [ISOLATE]: Implement accepted create/replace/removal and metadata handling primitives without production commands. Preserve unrelated hardlink names and later external data according to the contract.
  - Depends on: P2. Location: Linux guard backend and primitive tests.
  - Verify: native check→commit races, existing/missing target, hardlinks, changed-created-file deletion, full/read-only/cross-device storage, mode/owner/ACL/xattr policy.
  - Trap: using advisory locking as protection against all editors or applying in-place writes through a shared inode.
- P4 [ISOLATE]: Implement private staging/backup/temp permissions and cross-process resource lifecycle; run Linux conformance and unchanged Windows guard suite.
  - Depends on: P3. Location: primitive storage/cleanup, extracted comparison `Temporary` integration where required by privacy policy.
  - Verify: file/directory permission inspection, fd/lock contention/cleanup limits, full serial Cargo tests/clippy on both platforms and outside-sentinel fingerprints.

## Acceptance
- A1 (native fixtures): supported-envelope probes and all confinement/identity attacks → R1/I1/I2.
- A2 (native fixtures): replacement/removal/concurrency/hardlink/metadata matrix from accepted contract → R2/I1/I2.
- A3 (native fixtures + static): private storage and bounded descriptor/lock/cleanup ownership → R3/I2.

## Heavy authority and rollback
User owns native contract/dependencies/support scope; one writer/dedicated worktree, main session integrates. Verify fixture restore before writes. No app-facing Linux writes exist yet, so rollback restores owned backend changes and retains test evidence/backups. Stop on failed restore, outside-root mutation, unproven race/durability assumption, undocumented filesystem limitation or altered Windows behavior. No real-data mutation is authorized by a prototype success.

## Revision log
- 2026-10-02: Gated implementation packet; must be concretized after 08, not executed from a guessed Linux algorithm.

- [REV 2026-10-03] P1: owner approved all remaining work, necessary dependencies and useful improvements. Accepted08practical contract plus fresh reviewed-source run-03 restore/control results establish the Linux route. Exact signatures, support envelope, bounds and syscall obligations are in `.skillify/evidence/paperwing/12/implementation-plan.md`. Use a separate Linux-only backend while06Windows extraction remains deferred and unchanged. Adopt pinned Rustix1.1.5 fs only; no version/package upgrades. No app-facing Linux writes are enabled by12.

- [REV 2026-10-04] Implemented Linux guards and private comparison storage. Original review identified missing commondir observations, overstated retained-disk quota and blocking UI IPC. Repairs pass74/79 Rust tests and two fail-before controls. Native1000ms probe control blocks UI; fixed async dispatch renders68 frames and completes another IPC. Two native comparisons match baseline hashes/counters. Repair review was interrupted by model usage limit; acceptance remains pending. Primitive512MiB budget covers live memory only; persistent orphan accounting and hidden-metadata rollout decisions remain13–14 gates. No Linux writes enabled.

- [REV 2026-10-04] Independent Sol6.1/xhigh recheck approved all six repair files,16 final source hashes,32 evidence hashes and native controls. Approved as disabled-write primitives; hidden metadata and persistent orphan accounting remain explicit rollout gates. Packet13 handoff may proceed.
