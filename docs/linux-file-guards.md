# Linux file guards

Packet12 supplies Linux-only filesystem primitives and protected comparison reads.
Packet14 connects editor writes, directional copy, undo and recovery commands to
the durable journal and fresh authority checks. Per-root write probes must pass. Windows
files.rs and file_guard.rs retain their baseline bytes.

Packet15 also uses these guards for [Linux clone/reclone and desktop Trash](linux-folder-workflows.md).
These folder actions are available on eligible local ext4 roots; editor/copy/recovery commands remain disabled.

## Supported envelope

The initial write primitives support ordinary, user-owned local ext4 files and
directories with the current effective uid and gid. They require openat2, statx
mount identity, directory fsync and available inode flags. Read-only volumes,
mount crossings, linked paths, nested repositories, multiply linked files, special
permission bits, immutable/append/casefold flags and unsupported visible metadata
refuse writes. Reads remain available independently of the write policy. Linux root support
probes run in at most four blocking tasks, with immediate busy responses beyond
that limit. Cancellation of an IPC caller retains the task permit until work ends.

Regular-file mode, visible user.* attributes and POSIX access ACL bytes transfer
to the staged inode and are verified. Directory ACLs, including default ACLs,
are currently unsupported. New ordinary files use0644; private directories and
files use0700/0600. The code never mutates an existing destination inode in place.

Privileged or policy-hidden metadata is outside the supported envelope. An
unprivileged flistxattr call can omit inaccessible names. It cannot establish that
trusted.* or policy-hidden security.* attributes are absent. The guard rejects
unsupported names it sees; it cannot promise detection or preservation of hidden
attributes during inode replacement. This limitation requires an explicit rollout
decision before enabling user writes. It is not a verified fail-closed property.
See [Linux listxattr](https://www.man7.org/linux/man-pages/man2/listxattr.2.html),
[xattr namespaces](https://man7.org/linux/man-pages/man7/xattr.7.html) and
[ext4 attributes](https://kernel.org/doc/html/latest/filesystems/ext4/attributes.html).

## Ownership and mutation

Root retains its directory and protected metadata descriptors, including bounded
.git/commondir pointer bytes. Parent retains each destination ancestor. Fresh
current-namespace and held-identity checks precede reads, stages and mutations.
Descriptor ownership does not prevent another process moving those directories.

An existing destination publishes by atomic rename after final identity, byte and
metadata validation. A missing destination uses NOREPLACE. Created-file removal
validates its current bytes and identity, then unlinks it. Another writer can change
an existing file or move directories after the final check. That accepted practical
race remains; advisory flock does not exclude arbitrary editors. The separately
accepted [write contract](linux-write-contract.md) records this boundary.

Each stage uses a random0700 directory beside the destination, containing an
exclusive0600 content file. Supported destination metadata is applied inside that
private directory before publication. Parent and stage directories are synchronized.
A failure after rename or unlink returns MutationError.applied=true; callers must
retain recovery data and must not describe the operation as rolled back.

Private storage verifies namespace identity, ownership, permissions and one-link
files. Lock owns a nonblocking flock descriptor. Diff temporary materialization
uses these private primitives on Linux. Its cleanup deletes only held, unchanged
files and an empty verified directory. Changed or substituted artifacts are
retained privately. Cleanup failures never trigger recursive pathname deletion.

## Bounds

Handles are capped at512. Root clones share descriptors. Relative paths have at
most64 components; protected metadata locations are capped at8. Each file is at
most64MiB. Stages and private diff copies share a512MiB live-memory byte budget.
Linux diff materialization uses durable admission described below. The older private
Temporary primitive remains for existing guard tests and retained historical artifacts;
no automatic historical cleanup runs. Freeing a live-memory reservation does not free
retained disk storage.
Attribute names are bounded to64KiB/64 entries, with256KiB total values. Limits
return explicit errors; no authority-bearing resource enters a global cache.

## Verification

Native sacrificial fixtures first restore their known bytes and protect an outside
sentinel. Tests cover links, traversal, metadata aliases, root/ancestor replacement,
external edits, raced creates, final-check race controls, one-link policy, mode,
user attributes, kernel-validated access ACL transfer, private permissions, flock,
handle/byte limits and conservative cleanup.

A private user/mount namespace runs full64KiB tmpfs, read-only and cross-device
controls. ENOSPC, EROFS and EXDEV preserve the recorded source/sentinel; the host
mount namespace remains unchanged. Destination synchronization failure is injected
only in Rust tests after real native rename/unlink, proving applied outcomes and
changed bytes. This does not establish power-loss durability.

The actual comparison Temporary privacy regression fails with the old wrapper and
passes with guarded storage. Repair suites pass74 default/79 test-profile. The absent
commondir and synchronous runtime controls fail before their repairs. A native
1000ms stalled probe blocks rendering in the synchronous control; the fixed app
renders68 frames and completes a second IPC before the probe finishes. Two ordinary
native comparisons match the baseline fingerprint and every Git command count.
Diagnostic Clippy passes with dead_code permitted. Strict Clippy fails on unused
Linux-disabled backend code and staged guard APIs. Combined verification and the
independent frozen repair review pass.
Windows, actual older kernels, casefold filesystems, privileged metadata and real
power-loss checks remain unavailable. No real repository or recovery record is used.

Ignored evidence: .skillify/evidence/paperwing/12/. Revert only this slice's owned
files to disable its primitives; no production Linux write authority is enabled.

Combined-source verification on2026-10-04 passes80 default/85 test-profile tests,
63 frontend tests, Svelte check and production builds. Two native comparisons match
the baseline hash/counters. A fresh private credential drill passes with the reviewed
entropy/runtime repairs. The independent frozen guard repair recheck is approved.
No app-facing Linux writes are enabled.

## Durable comparison materialization

Linux comparisons configure the exact resolved app-data path without creating diff
storage during setup. The first materialization verifies current compared-root and
protected Git metadata snapshots before creating a missing app-data suffix or the
private `linux-diff-v1` namespace. Held no-link descriptors verify existing ancestors;
new components use0700 beneath a verified current-user-owned parent. Existing
permissions remain intact. Linked parents, stale roots, source overlap and ambiguous
same-device bind aliases refuse without a temporary-directory fallback.

Diff storage requires local ext4 with1024,2048 or4096-byte blocks. The initializer
checks the nearest existing prefix before creating anything. Other filesystems refuse.

Admission inventories durable checksummed reservations under a revalidated namespace
flock. Each allocation reserves both inputs plus143360 logical overhead bytes before
creating its random directory. Claims remain fully charged through interrupted creation,
cleanup and restart. The limits are1GiB and1024 reservations. Four materialization
leases own their work permits through cleanup. Input copies reserve the existing shared
512MiB temporary/stage memory budget before cloning. These limits bound logical content
and metadata accounting; they do not bound filesystem allocation or total process RSS.

Inventory prepays1MiB for namespace metadata plus4096 bytes for the lock. It keeps this
charge after cleanup and restart. Every new namespace entry requires64KiB of native
directory-growth headroom and verifies the observed growth afterward. Claims, allocation
directories and ready manifests are created under the namespace flock. A directory's
retained high-water size may exhaust namespace capacity before1024 reservations.
Unlinking entries does not imply that native directory size shrinks.

The namespace lock retries only native busy errors for at most5 seconds, at25-millisecond
intervals. Unsafe or replaced locks refuse immediately. Exclusive lock-create `EEXIST`
reopens and verifies the winning private file. Complete ready-manifest publication holds
the same inventory lock through verification and synchronization. Git runs after that
lock is released. Storage work and cleanup run on bounded blocking workers.

Cleanup verifies captured identities, exact contents and ordinary0700/0600 metadata.
It removes known files, their empty allocation directory, the outside manifest, then
the reservation last, synchronizing each affected parent. Changed or ambiguous artifacts
remain. Dropped or cancelled callers keep the same permits through independent bounded
cleanup. Shutdown without cleanup dispatch retains the durable charge and releases the
permits without running synchronous filesystem cleanup from Drop. No automatic eviction,
restart deletion or downgrade cleanup occurs. Unknown, unsafe or torn records block new
admission; operator inspection and any removal remain separately authorized work.
If native namespace growth or allocation overhead exceeds its bound, the owner retains
all artifacts and its claim. It releases work and memory permits without generic cleanup.

This storage slice does not enable application save/copy/recovery commands. Process-kill
fixtures establish interruption behavior; they do not prove power-loss durability.
