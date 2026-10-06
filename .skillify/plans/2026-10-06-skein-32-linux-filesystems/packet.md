# Packet 32 — Linux write support beyond ext4

| | |
|---|---|
| Status | Approved by the owner 2026-10-06 ("add multiple types support, btrfs, ext4, ntfs, fat32"). |
| Weight | Heavy (extends the packet 08 write contract). |
| Depends on | 14 (Linux file service). |

## Goal
Save, copy and recovery work on the filesystems people actually keep repositories on, with an honest per-filesystem capability instead of an ext4-only gate.

## Tiers (decided)
| Tier | Filesystems (statfs magic / fuse subtype) | Behaviour |
|---|---|---|
| Full | ext4, btrfs, xfs, f2fs | Same guarantees as ext4 after native drills pass. |
| Reduced | ntfs3 and ntfs-3g (fuseblk), vfat (FAT32), exfat | Writes allowed. Mode, ACL, xattr and owner preservation is skipped and reported in the recovery record; case-insensitive name collisions are checked before every create; durability uses file and directory fsync. |
| Refused | nfs, cifs/smb3, sshfs and other network or FUSE filesystems not listed, tmpfs/ramfs, overlayfs, squashfs, read-only mounts | Readable; writes refused with a reason naming the filesystem. |

## Requirements
- **R1** — Root probe detects the filesystem from `statfs` and `/proc/self/mountinfo` (fuse subtype, `ro`, case-folding flag) and returns its tier; the UI shows the tier and reason.
- **R2** — Every guard and journal operation that relies on a feature (RENAME_NOREPLACE, O_TMPFILE, flock, ACL/xattr, hard links, mode bits) has a capability check and a defined fallback or a refusal; no silent downgrade.
- **R3** — Case-insensitive volumes (vfat, exfat, ntfs, ext4/f2fs casefold dirs) refuse creating a name that collides case-insensitively with an existing entry.
- **R4** — Recovery records store which metadata was not preserved; undo restores what was saved and reports the rest.
- **I1** — Windows behaviour unchanged. ext4 behaviour unchanged.
- **I2** — Drills run only on loop-mounted sacrificial images (udisksctl loop-setup / mount, no root), never on real data.

## Steps
1. Capability model and detection; refactor the ext4 gate into the tier table. Check: unit tests on recorded mountinfo/statfs samples.
2. Feature fallbacks for reduced tier. Check: native tests per filesystem image (btrfs, xfs, f2fs if tools exist, ntfs3, ntfs-3g, vfat, exfat, ext4 casefold).
3. Native drill matrix per filesystem: save with BOM/CRLF, copy create/overwrite, external edit refusal, SIGKILL mid-save then recovery, undo, case collision. Check: one table of results per filesystem in docs/linux-write-contract.md.

## Done when
The drill matrix passes on every Full and Reduced filesystem image, refused filesystems refuse with a clear reason, and docs/linux-write-contract.md lists the measured guarantees per filesystem.
