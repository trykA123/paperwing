# Linux write contract: practical semantics accepted

Packet 08 is a research and prototype gate. Linux recoverable writes remain unavailable.
Packet 09 depends on this contract. Windows write code and recovery readers remain unchanged.

Packet15 implements guarded [clone/reclone and desktop Trash](linux-folder-workflows.md)
within these practical race limits. Linux recoverable file-write commands remain unavailable.

The existing source protects saves, filesystem copies and undo with Windows-specific handles
and transactions. Standard Linux resolution, rename and advisory-lock primitives do not
establish the same protection against concurrent writers or moved directories. This is a
bounded finding about the reviewed primitives, not a proof that every Linux mechanism is impossible.

## Reference environment and evidence

Research date: 2026-10-03. Reference host: Linux 7.2.4-3-cachyos, x86_64, uid1000,
local ext4 on `/dev/nvme1n1p1`, mounted `rw,relatime` at `/mnt/Sabrent`.
These observations establish one test environment, not support for every ext4 mount or kernel.

Prototype source lives in the dedicated `work/paperwing-08-prototype` worktree:
`/mnt/Sabrent/homelab/paperwing-08-prototype/prototype/linux-write-safety/probe.py`.
It creates a new marked0700 root, uses separate processes for external writers, and retains
all sacrificial files and recovery artifacts. No product write commands, real repositories,
credentials or existing journals are used. It verifies a byte-for-byte fixture restore first.

```bash
python3 prototype/linux-write-safety/probe.py --run \
  /mnt/Sabrent/homelab/paperwing-08-prototype/prototype/linux-write-safety/run-02
```

The runner passed independent pre-execution review. Native results and the reviewed
source fingerprint are recorded below. A green assertion can demonstrate a
protection gap; it does not mean the candidate write algorithm is safe.

## Threat and guarantee matrix

Windows entries describe implemented checks and existing tests. Native Windows execution
was unavailable in this session. Linux entries distinguish candidate obligations from proven properties.

| Boundary | Windows source and existing proof | Linux obligation and required evidence |
|---|---|---|
| Session and destination authority | [comparison write contexts](../src-tauri/src/compare.rs), [edit tickets and copy plans](../src-tauri/src/files.rs) revalidate registered roots, generation, endpoint and expected bytes | Preserve backend authority independently of UI capabilities. Test stale session/ticket/root and changed Git metadata at every check-to-commit boundary. Not implemented here. |
| Git coordination | [filesystem gate](../src-tauri/src/git/runner.rs) and clone busy checks exclude in-process Git/write overlap | Keep one gate and add a cooperating cross-process journal lock. Neither excludes unrelated editors. Probe same-UID writers that ignore locks. |
| Root and ancestor identity | [PinnedPath](../src-tauri/src/file_guard.rs) retains directory handles; `pinned_ancestors_block_substitution_and_reject_aliases` | Handle-relative confinement plus live descriptor identity. Holding a Linux FD must not be described as preventing ancestor/root movement. Probe moves while handles remain open. |
| Git metadata and worktree pointers | `files::lock_context` and `file_save` compare pointer bytes and pin metadata | Protect `.git`, `commondir`, linked-worktree locations and nested repositories. Probe metadata replacement after validation; repeated checks alone leave a race. |
| Path resolution | [ReadRoot](../src-tauri/src/paths.rs), native alias/link tests, `junction_escape_and_hardlink_mutation_are_blocked` | Reject traversal, links and mount escapes during resolution. `openat2` confinement does not lock the resulting name or its ancestors after open. |
| Existing-file replacement | `Journal::replace_authorized`, [Transaction](../src-tauri/src/file_guard.rs), `transaction_replaces_atomically_without_unlocked_destination_gap` | Conditional expected-byte/identity replacement and external-writer exclusion through commit. Ordinary rename does not supply this condition; test a writer after the final check. |
| Missing-file creation | Transactional create/replace; `atomic_move_never_overwrites_an_existing_destination` | Same-filesystem `RENAME_NOREPLACE` must preserve an external raced create and staged data. This property does not prove safe existing-file replacement. |
| Undo of overwritten files | `Journal::undo`, checksums/root identity, `atomic_replacement_restart_and_conflict_safe_undo` | Validate verified backups and current after-bytes, then restore conditionally. Rename/check gaps remain unresolved against non-cooperating writers. |
| Undo of created files | Transactional handle deletion; `created_file_undo_retains_new_streams_and_changed_root_is_refused` | Conditional removal of the expected object/bytes. `unlinkat` removes a name without an expected-inode/byte argument. Test replacement or mutation after validation. |
| Security metadata | `regular_stream`, `preserve_security`, `preserve_attributes`; named-stream and hardlink refusal tests | Define mode/uid/gid, ACL, xattr and security-label policy. Verify transfer or fail closed; reject unsupported ownership/links/metadata. A staged rename does not automatically preserve these values. |
| Recovery namespace and limits | `Journal::open`, `record_dir`, `used`, `list`;512MiB/1024 records, no eviction, pending-operation refusal | Separate platform/version namespace outside repositories;0700 directories/0600 backups, immutable state envelopes, bounded IDs/records/bytes. Same-UID tampering is not excluded by permissions. Limits/torn-state/corruption need production tests. |
| Crash/restart | `rollback_and_recovery_at_each_commit_boundary`, `restart_reconciles_prepared_and_replacing_states`, torn-state/backup verification tests | Persist backups, staged data and ordered records; synchronize files and affected directories. Kill/restart at every boundary, classify before/after/conflict/incomplete and retain artifacts. SIGKILL does not simulate power loss. |
| Batch and cancellation | Copy applies per file; cancellation stops between files; completed records survive | Preserve per-file results and partial-batch semantics. Never promise a batch transaction or cancellation rollback. Production tests remain in12–15. |
| Unsupported conditions | Windows refuses unsupported NTFS/TxF, links, streams and metadata | Probe actual root/filesystem/syscall support and refuse unavailable mechanisms. Exercise EXDEV, ENOSPC, EROFS, casefold and metadata failures; never enable fallback writes from a filesystem label. |

## Native primitive findings

`openat2` supplies resolution constraints including `RESOLVE_BENEATH`, `RESOLVE_NO_SYMLINKS`
and `RESOLVE_NO_XDEV`. It requires Linux5.6 or later; NO_XDEV also rejects bind-mount
traversal. The returned handle refers to an object, not an immovable pathname.
[openat2 manual](https://man7.org/linux/man-pages/man2/openat2.2.html),
[open manual](https://man7.org/linux/man-pages/man2/open.2.html).

`renameat2` supplies atomic missing-target creation with NOREPLACE and two-name exchange
with EXCHANGE. It does not document an expected-inode or expected-byte replacement condition.
EXCHANGE retains the displaced object but changes the destination; rollback can race another
writer. Filesystem support must be tested.
[rename manual](https://man7.org/linux/man-pages/man2/rename.2.html).

`flock` is advisory. A cooperating journal process can respect it, while another permitted
writer can ignore it. Leases have break deadlines and do not establish the required directory
rename/unlink exclusion.
[flock manual](https://man7.org/linux/man-pages/man2/flock.2.html),
[lease manual](https://man7.org/linux/man-pages/man2/F_SETLEASE.2const.html).

`unlinkat` has no expected-object/byte condition. File synchronization and directory
synchronization address different persistence boundaries. Filesystem/device power-loss
behavior requires additional evidence; ordered ext4 journaling is not a universal guarantee
of complete user-file data persistence.
[unlinkat manual](https://man7.org/linux/man-pages/man2/unlinkat.2.html),
[fsync manual](https://man7.org/linux/man-pages/man2/fsync.2.html),
[ext4 journal documentation](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html).

## Rust binding candidate

The lockfile contains Rustix1.1.5 and libc0.2.189 transitively. Rustix1.1.5 offers typed
`openat2`, resolution flags, `renameat_with`, rename flags, `OwnedFd`, flock, fsync and
unlinkat through its `fs` feature. Its published licenses permit Apache/LLVM-exception,
Apache-2.0 or MIT use. Nix0.31.3 also covers these primitives with its `fs` feature and
MIT license, but is absent from the lockfile.
[Rustix openat2](https://docs.rs/rustix/1.1.5/rustix/fs/fn.openat2.html),
[Rustix rename](https://docs.rs/rustix/1.1.5/rustix/fs/fn.renameat_with.html),
[Rustix metadata](https://docs.rs/crate/rustix/1.1.5/source/Cargo.toml.orig),
[Nix metadata](https://docs.rs/crate/nix/0.31.3).

Rustix is the preferred candidate for later review. No direct dependency was added or
feature graph changed. A binding does not strengthen the underlying native guarantee.
Adoption still requires review of the selected source/features and dependency authorization.

## Owner decision and implementation boundary

On 2026-10-03 the owner accepted practical Linux semantics and authorized packet09.
The accepted contract uses confined resolution, verified backups, expected-byte/identity
checks, cooperating journal exclusion, atomic per-file publication, private retained
recovery data and failure reporting. A concurrent writer or namespace change after the
final check may still cause save or restore to overwrite later bytes, or removal to
delete them. Linux must disclose this limit. Windows protection remains intact.
The strict alternative was investigated in the retained evidence below and was not selected.

The candidate Linux envelope starts with the tested local ext4 host. Foreign ownership,
unsupported links/metadata/mounts, missing syscalls, full storage and pending or corrupt
recovery refuse writes. Other kernels/filesystems require their own evidence. The namespace
proposal is a Linux-specific versioned recovery store under application data, separate from
Windows `recovery-v2`; no journal is translated or migrated.

Packet09 may define native paths, physical identity and typed per-root capability reasons
after the owner accepts the contract. Its write capability remains false. Reading comparison
content must be independent of acquiring write tickets.

The accepted handoff for later packets is bounded as follows. These are implementation
obligations, not implemented services or proven guarantees:

| Packet | Boundary and selected mechanism | Gate before enabling it |
|---|---|---|
| 12 | Linux root/parent handles with `openat2` confined resolution; identity/link/ownership/metadata checks. Stage on the destination filesystem. Use NOREPLACE for a missing target, ordinary atomic rename for an existing target after final validation, and unlink for created-file undo after validation. Do not claim conditional replacement/removal or use EXCHANGE rollback to hide a conflict. | Native link, moved-directory, external-writer, metadata, mount and I/O tests must document the accepted race windows and fail closed outside the supported envelope. |
| 13 | Separate `linux-recovery-v1` store under application data;0700 directories,0600 artifacts, one cooperating process lock. Verify backups before publication; synchronize artifacts and state boundaries; retain incomplete or ambiguous records. Preserve Windows `recovery-v2` without translation. | Corruption, torn state, size limits, lock contention, storage exhaustion and process-death tests; no power-loss claim from SIGKILL alone. |
| 14 | Reuse comparison ownership/generation checks and fresh backend tickets. Validate expected identity/bytes and metadata immediately before each publication. Return per-file applied/failed/notAttempted results and retain verified displaced data for recovery. Capabilities remain descriptive. | Native save/copy/undo failures and conflicting writers, plus disclosure of the practical Linux contract. Recovery must preserve a later edit detected before its final check. |
| 15 | Rebind every folder operation to saved ownership; use native physical identity for existing destinations and sharing. Preserve data/configuration on failed removal, validate clone/reclone authority again at mutation, and use only proven root/backend capabilities. | Native case/linked-root/sharing/reclone/trash failures and configuration retention. Unsupported mounts or unsafe identities refuse mutation. |

Rustix remains the preferred binding candidate; adding its direct dependency is a separate
reviewed change. The namespace name and algorithm choices above require no migration now.
Exact production Rust signatures follow the owning packet's implementation review.
Packet09 exposes native paths and typed support, while all Linux recoverable writes remain
unavailable until these later packets meet their gates.

## Native probe results

Fresh `run-02` completed on 2026-10-03. Report:
`/mnt/Sabrent/homelab/paperwing-08-prototype/prototype/linux-write-safety/run-02/report.json`.
Reviewed and executed source SHA256:
`12255ad9fd323e02210b5d0da433eb4cdc7a36c31cd734c0cb03d715db6b7982`.
The runner records 11 verified observations, seven reproduced gaps and two unverified probes.
These counts describe fixture observations, not eleven production safety guarantees.

| Observation | Native evidence | Limit |
|---|---|---|
| Restore and harness failure cleanup | Restored fixture hash; stalled, malformed and partial-output children killed/reaped with closed pipes in at most 0.253s | Own fixture and processes only |
| Resolution | Unprotected symlink changes owned sentinel; `openat2` refuses symlink/traversal and preserves restored sentinel | No protection against later directory movement |
| Held directories | Separate processes move parent, root and metadata; held descriptors still write moved directories | Three protection gaps reproduced |
| External writer | Same-UID writer changes the file despite exclusive `flock` | Advisory locking gap reproduced |
| Existing replacement | Writer changes bytes after the last check; ordinary rename overwrites them | Conditional-replacement gap reproduced |
| Missing creation | NOREPLACE returns EEXIST; later bytes and staged bytes survive | Same-filesystem create-only property |
| Exchange and rollback | First exchange retains displaced bytes; second writer changes destination; rollback installs older bytes and retains newest bytes under staging | Destination conflict protection remains unestablished |
| Created-file undo | Writer changes bytes after validation; unlink removes those bytes | Conditional-removal gap reproduced |
| Hardlinks and privacy | Unprotected hardlink edit mutates alias; link count is detectable; private directory/file modes are0700/0600 | No race-free link identity or same-UID tamper exclusion |
| Metadata | Naive replacement loses attributes; explicit fixture transfer preserves mode, user xattr and POSIX ACL | Foreign uid/gid and security labels unverified |
| Journal lock | Cooperating second process is refused; closing the lock permits reacquisition | Cooperating processes only; no mandatory editor exclusion |
| Crash checkpoints | Eight children killed at durable-state boundaries; retained backups hash-checked and current bytes classified | Illustrative fixture protocol, not production journal implementation |
| Mount/I/O failures | Private user/mount namespace proves EXDEV with unchanged source, bounded ENOSPC and read-only EROFS | tmpfs failure controls; no network/overlay filesystem support claim |
| Case behavior | `Folder` and `folder` have different inodes inside the fixture directory | Casefold flag unsupported on this mount; casefold remains unverified |
| Power loss | No appropriate power-loss experiment ran | SIGKILL alone cannot establish this guarantee |

Crash classifications: `firstBackup` and `backups` retain incomplete artifacts; `prepared`,
`staged` and `replacing` classify as not applied; `renamed`, `directorySynced` and `applied`
classify as applied. A later external edit is retained at every checkpoint. The prototype
does not attempt automatic restore or deletion after these edits.

The earlier `run-01` remains retained. Its combined mount probe failed because the utility
reused an unmapped uid during remount. An isolated diagnostic identified the cause; explicit
tmpfs source/type corrected it, passed focused independent review and enabled `run-02`.
No old report was rewritten and no host mount changed.

No strict Linux write contract is established. Complete production recovery parsing,
torn-state/corruption/limits tests, foreign ownership/security labels, casefold, power-loss
behavior and the supported envelope remain explicit gaps. Final independent evidence review
approved this research record for the owner decision. The owner subsequently accepted
practical semantics; strict exclusion remains unproven and is not claimed by this contract.

## Strict-contract continuation

Further investigation on 2026-10-03 retained the strict guarantees. No reviewed mechanism
closes both content-write exclusion and namespace-movement exclusion for this ordinary
uid1000 ext4 environment. This finding is limited to the mechanisms below.

| Mechanism | Official contract | Reference-host evidence and conclusion |
|---|---|---|
| File leases | Owned regular files can receive leases. Conflicting opens/truncates request a break; the kernel eventually forces release. Directories cannot receive file leases. | Leases enabled; configured break time45s. Native write lease blocked an external open for0.25s, delivered SIGIO and allowed completion after explicit release. Separate processes replaced/unlinked the leased name and moved its parent while the lease remained held. Forced expiry was not tested. |
| Immutable/append-only flags | Setting or clearing requires `CAP_LINUX_IMMUTABLE`. Existing open descriptors are a separate limitation. | Effective capabilities zero. Setting immutable on an owned fixture returned EPERM; original flags and bytes survived. Privileged setting and preexisting-descriptor behavior were not tested. |
| Mandatory record locks | Linux removed support from5.15 onward. Ordinary record locks remain advisory. | Reference kernel7.2.4 is outside the supported versions. No legacy mandatory-lock setup attempted. |
| Fanotify permission decisions | Content permission groups require `CAP_SYS_ADMIN`; namespace notifications do not supply a rename/delete permission gate. Some mapped-memory changes are outside event coverage. | Permission-group initialization returned EPERM with zero effective capabilities. No filesystem marks or permission monitor were installed. |
| Filesystem freeze | Freeze stops filesystem modifications, including the app's publication writes, until thaw. The upstream ioctl requires `CAP_SYS_ADMIN`. | Host filesystem freezing was not attempted. It does not supply an ordinary-user per-file writable commit boundary. |

Sources checked on2026-10-03: [file leases](https://man7.org/linux/man-pages/man2/F_SETLEASE.2const.html),
[inode flags](https://man7.org/linux/man-pages/man2/FS_IOC_SETFLAGS.2const.html),
[chattr limitations](https://man7.org/linux/man-pages/man1/chattr.1.html),
[record locking](https://man7.org/linux/man-pages/man2/fcntl_locking.2.html),
[fanotify initialization](https://man7.org/linux/man-pages/man2/fanotify_init.2.html),
[fanotify events and limits](https://man7.org/linux/man-pages/man7/fanotify.7.html),
[filesystem freeze](https://man7.org/linux/man-pages/man8/fsfreeze.8.html),
[upstream freeze ioctl](https://code.googlesource.com/linux/torvalds/linux/%2B/78273df7f646f8daf2604ec714bea0897cd03aae/fs/ioctl.c).

The supplemental runner passed independent pre-execution review and completed in a new
marked0700 root. It uses the unchanged reviewed helper from `run-02` and leaves all artifacts
available for inspection:

```bash
python3 prototype/linux-write-safety/lease_probe.py --run \
  /mnt/Sabrent/homelab/paperwing-08-prototype/prototype/linux-write-safety/lease-run-01
```

Runner SHA256: `8b5ec8c0a20017232ab6d7d15db315fcd9b422528e317d15219ca6b54d61dcd2`.
Report: `lease-run-01/report.json` beside the runner. Counts: two verified observations,
three namespace gaps and two unsupported privileged mechanisms. Root independently
verified report/helper fingerprints, fixture bytes, moved/replaced/missing names, private
mode and reported child cleanup. Final independent review approved this supplemental
research record and its blocked status for presenting the owner options.

A privileged mediation service or a different filesystem/trust boundary would change the
deployment contract and need separate design, authorization and native proof. These are
future investigation directions, not accepted mechanisms. The subsequent owner decision
selected the practical contract and bounded handoff above; packet09 may proceed.
