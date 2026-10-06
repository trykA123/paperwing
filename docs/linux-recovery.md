# Linux durable recovery library

Packet13 adds a native Linux library under `src-tauri/src/linux_journal`.
Packet14 connects application save, copy, undo and recovery IPC through `linux_files`.
Writes require a successful native root probe.
Windows source and `recovery-v2` records remain unchanged. No record is translated.

## Storage and ownership

`Journal` owns a private `linux-recovery-v1` directory and its nonblocking flock.
Production storage requires ordinary, user-owned, writable local ext4. Directories
use0700; exclusive backup, intent, state and cleanup files use0600 with one link.
Fresh descriptors verify identity, permissions and supported visible metadata.
Advisory flock excludes cooperating journal processes, not arbitrary editors.

Before publication, the store and its freshly reopened namespace ancestors are
compared with the fresh target root by device and inode. This catches differently
spelled roots and whole-root aliases without using lexical containment as authority.
Traversal is bounded to64 store ancestors; changed or unavailable observations refuse.
Same-device store/root pairs with different statx mount IDs also refuse because their
separation is unproven. Otherwise separate bind mounts on the same device therefore
fall outside this supported envelope. This is conservative admission, not general
physical ancestry detection.

Targets use the existing [Linux file guards](linux-file-guards.md). Their parents must
already exist. Missing nested destination parents and their journal linkage belong to
packet14. Files support ordinary permissions, the current effective uid/gid, visible
`user.*` attributes and POSIX access ACL bytes. Unsupported ownership, links, special
bits, mount crossings, inode flags and visible security metadata refuse mutation.
Privileged or policy-hidden metadata remains outside the primitive envelope and is a
packet14 rollout decision. Checks cannot prove inaccessible attributes absent.

## Immutable records and limits

Each random `r-` record contains verified complete `before` and `after` backups,
immutable `intent.json`, and exclusive append-only `state-NNNN.json` revisions.
Linux/version1 envelopes bind canonical serialized payloads with SHA256. Revisions
bind the exact intent and previous revision. Readers verify the entire legal chain;
they never fall back past a corrupt latest revision. Unknown fields, variants,
versions and platforms remain foreign. Torn, missing or corrupt known data remains
incomplete. Both categories retain artifacts and refuse automatic mutation.

Intent binds root identity, original protected-metadata inputs and observations,
relative destination, ancestors, before identity/security/content, intended after
security/content and any reverse-record linkage. These serialized values are evidence.
Undo reconstructs fresh guards; values alone never authorize a native operation.

Bounds are64MiB per file,512MiB retained accounted storage,1024 records,4096 revisions,
64 destination components,8 protected metadata locations and512 live handles. Attribute
bounds are64 names,64KiB encoded-name input,64KiB per value and256KiB total values.
Persistent accounting includes private artifact lengths, directory sizes, stage file
lengths and supported stage metadata. It reopens one parent at a time and retains only
bounded value descriptors. Parent enumeration is capped at1024 entries, including
ordinary siblings; larger directories refuse conservatively. Accounting is a logical
artifact budget, not an allocated-block filesystem quota. No automatic eviction occurs.
The live byte permit is not persistent disk accounting.

JSON files are capped at8MiB. A conservative combined cleanup-envelope bound includes:

| Encoded component | Worst-case allowance |
|---|---:|
| Two security snapshots: decimal byte arrays and escaped attribute names | `2 * (4 * 256KiB + 6 * 64KiB)` =2.75MiB |
| Root,8 input paths,8 protected paths and8 pointer byte arrays | 548864 bytes |
|63 escaped4096-byte ancestor paths | 1548288 bytes |
|1024 fixed provenance edges at768 bytes each | 786432 bytes |
|4100 fixed cleanup artifacts at320 bytes each | 1312000 bytes |
| Remaining fixed fields, identities, destination and framing | 65536 bytes |

The combined allowance is7144704 bytes, below8388608 bytes. Actual serialization is
checked before persistence. Initial admission reserves both backups, staged after
bytes, the actual encoded intent, stage metadata and128KiB framing space. Every later
revision or cleanup manifest separately reserves its actual encoded size plus4096 bytes
before allocation. Large provenance cannot bypass the512MiB budget.

Stages have random private sibling directories. Prepared state names the candidate
before its creation. Staged state persists exact directory/file identity before
Replacing state and rename. Verified retained stages count after restart. Unproved,
changed or unowned candidates block new publication and remain retained. Diff temporary
orphans outside the journal need packet14 ownership and reconciliation; this library
does not invent a disk ledger for them.

## Publication and restart

Publication verifies and synchronizes backups, intent, initial state and the store
parent before staging. Stage data and supported security are verified before Replacing.
Native replacement uses atomic rename; missing-target creation uses NOREPLACE.
Applied state records the actual staged identity. Errors preserve record identity and
whether native publication occurred, including failures after rename or unlink.
`Published` retains lossless after identity and security for the backend caller.

Normal new publication requires every inventory record to be a verified terminal
Applied, Undone, NotApplied or Resolved record before any new allocation. Pending,
Conflict, incomplete, foreign and unaccountable artifacts refuse admission. A pending
record remains an admission blocker for that call even if reconciliation resolves it.
Reverse publication has a recovery-only exception bound to a freshly verified original
Undoing revision and its exact reverse ID. It cannot bypass quotas or unsafe accounting.
There is no generic bypass flag.

| Publication interruption | Honest restart result |
|---|---|
| First backup, both backups or intent before state | Incomplete; retain backups |
| Prepared before candidate creation | NotApplied if current before identity/content/security matches |
| Candidate without persisted identity proof | Retain; refuse unproved stage cleanup and new writes |
| Staged or Replacing before rename | NotApplied if verified current before identity matches |
| Rename, directory sync or Applied | Applied only when verified staged identity/content/security matches |
| Any boundary followed by unrelated target changes | Conflict or incomplete; preserve current bytes |

Equal bytes alone never establish identity. Reconciliation checks fresh root, protected
metadata, ancestors and target state. Directory-handle ownership cannot prevent later
namespace movement. The accepted [practical write contract](linux-write-contract.md)
permits a non-cooperating writer or namespace change after the final check to race
rename or unlink. This library does not claim strict conditional replacement.

## Reader, export and conditional undo

Listing preserves valid unrelated records beside corrupt or foreign entries. Verified
backup export needs a known valid intent and the requested complete backup, but does
not require a valid state chain or a currently matching root. Unknown intent or changed
backup bytes refuse export. Retained statx mount identities may refuse automatic undo
after an OS remount or reboot. Verified export remains available when root identity
refuses, within the original supported ownership/metadata context.

Existing-file undo checks the exact after identity, content and security with fresh
guards, then journals a linked reverse replacement restoring the before bytes and
supported security. Created-file undo persists Undoing before verified unlink. Repeated
terminal undo leaves bytes unchanged. Conflict acknowledgement requires confirmation,
persists Resolved and retains the current target and backups.

Sequential saves can undo through app-owned reverse transitions. Each provenance edge
binds a verified Undone original, its Applied linked reverse, both intent/state hashes
and exact source/destination identities in the same root, ancestor, content and security
context. Undoing persists the selected identity and bounded edge chain. Verification
streams backups rather than loading every record together. Missing, cleaned, corrupt,
competing or cyclic dependencies refuse older undo. An external same-byte replacement
still refuses. Packet14 must protect transitive provenance dependencies while exposing
an active undo stack.

An original whose linked reverse is itself Undoing requires explicit recovery of that
linked record first. It does not recursively load nested pending records or mutate the
target. Valid unrelated reader/export operations remain available. Unsafe or changed
stage context can still block mutation because storage accounting must remain honest.

## Confirmed resumable cleanup

Cleanup requires explicit confirmation and verified Undone, NotApplied or Resolved
state. Applied, pending, incomplete and foreign records are retained. A checksummed
`cleanup-r-….json` authority lives outside the record directory and binds the terminal
intent/revision, exact directory identity and every artifact name/identity/length/hash.
All surviving inventory is verified before any deletion. New, changed or linked
artifacts refuse cleanup. No recursive traversal occurs.

Cleanup removes only verified owned stages and files, then the verified empty record
directory. It synchronizes the store parent before removing the outside authority last.
The manifest survives record-directory removal and authorizes restart resumption.
Missing declared artifacts are accepted only during that confirmed phase. Removing the
final authority is idempotent when the record is already absent. Results report actual
removed artifacts, completion and partial warnings. Capacity returns only after owned
artifacts physically disappear.

## Native verification and rollback

The final library matrix passes114 default and119 test-profile tests, with one separately
discovered ignored subprocess helper. The focused suites pass15 guard and31 journal
tests; the journal suite also discovers that one helper. Full suites invoke46 helper
processes:41 pidfd-owned SIGKILL/restart cases, one flock competitor, two native I/O
controls and two private-namespace bind controls. SIGKILL cases cover10 publication,
14 reverse/created undo,11 stacked undo and6 cleanup boundaries. Cleanup includes record
directory removal and final outside-manifest removal, with resumption and reclaimed
capacity assertions.

Private user/mount namespaces reproduce actual journal ENOSPC, private-file EROFS and
EXDEV while preserving the source and outside sentinel. Production tmpfs admission
remains disabled. Marked fixtures prove backup restoration before mutation, including
fresh identity and supported metadata. Semantic fail-before controls cover stack undo,
nested pending reconciliation, deep-parent handle accounting and outside-root admission.
The three unsafe outside-root before controls actually published only inside marked
fixtures; authorized fresh outside journals restored exact backup bytes and security.
Original records, backups, restored-file proofs and sentinels remain available.

The stage-lifetime test assumption was corrected: ordinary stage Drop already removes
its empty directory before the directorySynced checkpoint. The earlier renamed
checkpoint can retain that empty stage. Persisted candidate/proof checks remain tested.

Diagnostic Clippy passes with dead_code permitted. Strict Clippy is recorded separately
and still reports unused disabled/staged backend APIs. The only dependency change is the
already locked Linux direct sha2=0.10.9 edge; all553 locked package tuples remain intact.
Evidence and explicit source snapshots/hashes live under ignored
`.skillify/evidence/paperwing/13/`. Native Windows, reboot, power-loss, older kernels,
casefold filesystems and hidden privileged metadata were not tested. Process restart
and SIGKILL evidence do not establish reboot or power-loss durability.

Rollback disables new writers while preserving a forward reader/export/undo for these
Linux records. Source rollback cannot translate existing records into Windows format.
Packet14 must add fresh registered-root/settings/session/editor/ticket authority, nested
parent creation, dependency-aware cleanup, diff-orphan accounting and the hidden-metadata
rollout decision before app-facing writes. Independent review of the frozen packet13
implementation remains a gate; these results are implementation evidence, not self-approval.

## Independent review repair1

The review of the original frozen packet found two implementation defects. Resumed
created-file Undoing could reach unlink without checking that the held flock file still
owned the current lock name. Child pidfd setup could panic before cleanup ownership;
a checkpoint exit could reap without bookkeeping and then panic again during Drop.

Undo now revalidates the held lock at entry and immediately after final target validation
inside the removal primitive. Native entry and final-boundary controls retain the old
lock, hold a replacement Journal lock and prove refusal with applied=false, identical
target identity/content/security and identical record artifact bytes. Both controls
performed native unlink before the repair and retained their complete backups.

Subprocess ownership now starts immediately after spawn. Pidfd setup is fallible;
failed acquisition terminates and reaps the owned child. Every observed exit updates
reaping and cached status. Drop performs bounded cleanup without assertions or panic.
Externally owned sacrificial subprocesses reproduce the old acquisition leak and early
exit abort. The parent holds its helper pidfd and acts as an explicit subreaper for the
one declared descendant. It captures the descendant PID, terminates/reaps an orphan
when required and verifies no children remain. The repaired controls need no external
termination and preserve their fixture sentinels.

Repair runs write exclusively under `.skillify/evidence/paperwing/13/repair-1/`.
The original frozen manifest, source snapshots, native fixtures and four SIGKILL JSON
matrices remain unchanged. The repair adds four native regressions; final results are recorded separately as
118 default/123 test-profile and15 guard/35 journal passes, with one ignored
helper. Each full suite invokes50 exact native helpers and two additional external fault owners;
the original41 SIGKILL checkpoints remain covered. Actual final command results and
hashes are recorded in the separate repair freeze. Strict Clippy remains a recorded
unused-code failure. Independent repair recheck is required before integration.

## Guarded parent library

The packet14 library adds value-only missing-parent previews and durable parent records.
New parent operations require `Journal::open_guarded`
with actual source roots and an outside local-ext4 storage location. `open_existing` creates
nothing and supports private recovery without a surviving source root.

`replace_with_parents` accepts an exact preview, file snapshot and two-phase caller authority.
Slow bounded refresh precedes fresh native checks. Fast revocation follows exact parent/file
record and flock checks immediately before each exclusive mkdir or file publication. Existing
file record encodings and unbound APIs stay unchanged. An equal-name directory created by
another operation is refused; the library never adopts it into an earlier preview.

Each `p-` record contains a bounded intent and immutable checksummed state chain. Every intent,
worst-case future revision and full outside cleanup proof must fit64KiB before allocation.
At most63 missing parents are supported when that encoding proof fits. The immutable charge
is `(2*n+10)*65536+4096` bytes, shared with file records under512MiB/1024 logical records.
Unsupported encodings and unknown artifacts remain and block new admission. Linked parent
records preserve historical directory creation after later file undo or record cleanup.

`list_parents` returns separate typed parent rows and observed file-reference status.
Restart classifies current observations without creating directories or resuming publication.
Unproved mkdir results become conflicts with an uncertain index. Errors retain the parent ID
and only the durably recorded created prefix; file errors retain their separate file ID and
honest applied outcome, including failures after successful publication.

Confirmed parent acknowledgement resolves a verified conflict without target mutation.
Confirmed cleanup deletes only frozen private record files and the empty record directory.
Its complete `parent-cleanup-p-<id>.json` proof is prepaid, verified and synced before deletion.
The full reservation remains charged while the directory or proof exists. Every removal
rechecks exact artifacts and flock authority; the outside proof is removed last. The cleanup
`removed` value counts record files, excluding its administrative proof and directory.
Destination directories and referenced file records remain intact.

Owned native controls cover63 actual created identities, encoding/capacity limits, authority
substitution, honest post-mkdir/publication failures, restart classification and cleanup.
A76-case SIGKILL matrix checks exact stages, uncertain indices, full target subtrees, source,
sentinel and child reaping. The separate frozen13 reader harness exercises foreign parent
artifacts, verified file export and conditional create/replace undo in four real layouts.
Independent implementation review and owner acceptance remain gates. SIGKILL results do not
establish power-loss durability or native Windows behavior.

## Application file service

Packet14 exposes the Windows command names and response shapes through `linux_files`.
Linux persistence remains separate; the reserved Windows `rootVolume` and `rootIndex`
response fields are zero on Linux. Native journal root snapshots authorize recovery.

Edit tickets retain exact identity, supported metadata and expected bytes. Limits are32
open tickets and2MiB per file. Four previews each hold at most128 files and32MiB of
source/destination snapshots. Four blocking workers retain ownership after dropped IPC.
Close, cancel and page reload revoke authority. Comparison refresh revokes its generation.

Every mutation refreshes registered settings and native root/source authority. Final
publication checks revocation after journal, parent and destination verification. A batch
may reuse only parent identities recorded by its own earlier file publications. Failure
returns applied/failed/notAttempted outcomes and retained recovery identities.

Recovery opens existing storage without creating it. Parent rows expose historical
creation and retain destination directories. Acknowledgement and cleanup operate only
on verified eligible journal records; incomplete, foreign and unresolved backups remain.
Close editors referencing file undo records before cleanup. A missing or replaced root
refuses automatic undo; listing and verified private acknowledgement remain accessible.
The practical external-writer race and hidden-metadata limits remain unchanged.
