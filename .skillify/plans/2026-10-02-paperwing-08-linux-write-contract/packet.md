# Packet 08 — prove and accept the Linux write protection contract

**Status:** Practical contract accepted by the owner; prototype evidence independently reviewed. **Weight:** Heavy; design/prototype packet, NOT production write implementation. **Depends on:** accepted 01.
**Read first:** README safety/recovery contract, `file_guard.rs`, `files.rs::Journal`, `paths.rs::ReadRoot`, comparison write-context validation and working-plan Heavy gates.

## Outcome and scope
An accepted, evidence-backed Linux protection/storage contract and bounded implementation handoff for 12-15. Match Windows user-facing workflows without pretending Linux has TxF. Preserve existing Windows protection. In: native API/filesystem research, sacrificial prototypes and adversarial tests. Out: enabling Linux commands, changing Windows guarantees, migrating real journals or choosing a weaker contract silently.

## Requirements and invariants
- R1: Specify supported kernel/filesystems/mounts, handle/path identity, replacement/create/removal, external-writer semantics, security metadata, crash durability, locking and recovery format/namespace.
- R2: Demonstrate each claimed guarantee with positive-control attacks and interruption/restart probes; identify guarantees that cannot be established.
- R3: User accepts the contract and exact module/API/test handoff before any 12-15 implementation.
- I1: No real data/credentials or product write paths touched; no ordinary rename/advisory-lock equivalence claim without evidence.
- I2: Existing Windows NTFS/TxF guarantees and record readers remain unchanged; unsupported conditions fail closed with retained recoverable data.

## Evidence
- [FACT] Windows uses TxF open/replace/commit, share-mode exclusion, pinned directories, verified backup/state records and conflict-safe undo.
- [FACT] Process-local `filesystem_gate` does not establish cross-process exclusion. Linux prototype mechanism and filesystem guarantee are not established by this source.
- [DECISION] Same features is the goal; OS-specific safety/durability differences must be explicit accepted decisions. If a required guarantee is impossible on the reference filesystem, stop and present options instead of declaring parity or leaving a read-only final product.
- [ASSUMPTION] A practical supported Linux envelope can satisfy an owner-accepted contract. P2/P3 can disprove it; no backend algorithm has been selected.

## Steps
- P1 [ISOLATE]: Write a threat/guarantee matrix mapping every current save/copy/undo/recovery check to Linux obligations, current Windows proof and required proof type. Include non-cooperating writers and ancestor/metadata replacement across every check→commit gap.
  - Depends on: none. Location: proposed `docs/linux-write-contract.md`; existing guard/journal/ticket tests.
  - Verify: independent review names each gap and links original code/tests; no unsupported Linux API assertion is treated as fact.
- P2 [ISOLATE]: Research the installed kernel/native APIs and maintained Rust bindings from official/pinned sources; compare candidate handle-relative confinement, same-filesystem replacement, safe deletion, synchronization and locking mechanisms.
  - Depends on: P1. Location: proposed contract and ignored `prototype/linux-write-safety/` evidence.
  - Verify: dated source citations plus local capability checks on a disposable root. Confirm actual mount/kernel; never run fetched example code or adopt a crate without source/license/version review and dependency approval.
  - Trap: assuming no-follow/canonicalization pins ancestors, device+inode alone defeats all identity reuse, advisory locks constrain unrelated editors, or rename creates a conditional compare-and-swap.
- P3 [ISOLATE]: Build reviewed sacrificial prototypes and deterministic multi-process attacks/interruption probes. Cover existing/missing file races, moved ancestors/root/metadata, symlinks/hardlinks, changed file before undo/unlink, cross-device/full-disk/read-only/unsupported mounts, owner/mode/ACL/xattr policy, second journal process and private backup/temp permissions.
  - Depends on: P2. Location: ignored prototype plus packet-01 fixture runner.
  - Verify: versioned prototype runner with documented exact commands and positive controls; process-kill/restart drills around every durable state transition; assert outside sentinel bytes and later external writes are protected according to the stated contract.
  - Fails if: an attack never succeeds in its unprotected control, a test only covers cooperating clients, or process-kill tests are misreported as proof of power-loss behavior. Power-loss/directory-durability claims require additional appropriate evidence.
- P4 [ISOLATE]: Select supported envelope and native primitive/journal boundary from evidence; specify error/capability reporting, fail-closed cases, per-file versus batch scope, record compatibility and forward recovery rollback. Amend 12-15 with exact signatures/mechanism/tests, and submit unresolved differences to the user.
  - Depends on: P3. Location: contract, working plan open gate, dependent packets.
  - Verify: independent reviewer approves evidence fit; user explicitly ratifies any accepted risk/OS difference before the production implementation handoff.
  - Fails if: a generic `Transaction` abstraction claims unavailable Linux semantics or a design silently relaxes Windows protection.

## Acceptance
- A1 (static + native fixtures): complete cited contract and reproducible capability/attack matrix → R1/R2/I1/I2.
- A2 (native fixtures): attack controls, crash/restart/conflict/removal/metadata/locking probes substantiate each claimed property, with limitations explicit → R2/I1/I2.
- A3 (owner-observed + static): accepted contract and amended bounded 12-15 handoffs, or explicit blocked status → R3/I2.

## Heavy authority and rollback
User owns supported-envelope and protection-risk decisions. Dedicated prototype worktree, one writer; main session integrates documentation only until production packets are approved. Verify restoring a sacrificial fixture before probes. Prototype rollback removes only manifest-owned sacrificial outputs and leaves every recovery artifact needed for inspection. Stop on failed restore, unexpected outside-root writes, impossible safety claims, missing native proof or unaccepted tradeoffs. No point of no return on real data is authorized.

## Revision log
- 2026-10-02: Proposed proof/decision gate; exact Linux primitive implementation remains deliberately unchosen.
- 2026-10-03: P1–P3 evidence recorded in `docs/linux-write-contract.md` and isolated prototype run-02. Seven native protection gaps reproduced; casefold/power-loss remain unverified. P4 and dependent09 are blocked pending explicit owner contract decision and accepted bounded handoff. No production Linux writes enabled.
- 2026-10-03: Strict-contract continuation researched leases, immutable flags, mandatory locks, freeze and fanotify. Reviewed lease-run-01 reproduced namespace changes despite leases and EPERM for privileged mechanisms. No strict ordinary-user candidate established; owner decision and dependent handoff remain open.

- [REV 2026-10-03] Owner accepted practical Linux semantics and authorized packet09. See `docs/linux-write-contract.md` for the disclosed concurrent-writer limits and bounded12–15 handoff. No Linux write backend was enabled.
