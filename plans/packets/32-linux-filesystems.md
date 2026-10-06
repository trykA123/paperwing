# 32 — Linux write support beyond ext4

Status: parked (Windows first); blocked by nothing technical, start only when the owner unparks it
Platform: Linux only; Windows and ext4 behaviour must not change
Size: L
Role: api-builder (gpt-6.1-sol xhigh)

## Goal
Save, copy and recovery work on the file systems people keep repositories on, with an honest capability per file system instead of an ext4-only gate.

## Already done
- Linux file service on ext4: `src-tauri/src/linux_files/`, `linux_guard/`, `linux_journal/`, `linux_diff/`, with the contract in `docs/linux-write-contract.md`.
- Writes are refused outside ext4 today. Root probing and the support reason reach the UI through the platform capability (`src-tauri/src/platform.rs`, `src/lib/platform.ts`).
- Missing: file system detection beyond ext4, the tier table, feature fallbacks, the drill matrix.

## Decisions
| Tier | File systems (statfs magic or fuse subtype) | Behaviour |
|---|---|---|
| Full | ext4, btrfs, xfs, f2fs | Same guarantees as ext4 after the native drills pass. |
| Reduced | ntfs3, ntfs-3g (fuseblk), vfat (FAT32), exfat | Writes allowed. Mode, ACL, xattr and owner preservation is skipped and reported in the recovery record. Case-insensitive collisions are checked before every create. Durability uses file and directory fsync. |
| Refused | nfs, cifs/smb3, sshfs and other unlisted network or FUSE file systems, tmpfs, ramfs, overlayfs, squashfs, read-only mounts | Readable. Writes refused with a reason naming the file system. |

- No silent downgrade: every feature a guard or journal operation relies on (RENAME_NOREPLACE, O_TMPFILE, flock, ACL and xattr, hard links, mode bits) has a capability check and either a defined fallback or a refusal.
- Case-insensitive volumes (vfat, exfat, ntfs, ext4 or f2fs casefold directories) refuse a name that collides with an existing entry.
- Undo restores what was saved and reports what was not preserved.
- Drills run only on loop-mounted sacrificial images, never on real data.

## Scope
- Do: detection, tier table, capability checks, fallbacks, native drills, doc table.
- Do not: touch Windows code; weaken ext4 guarantees; run drills on real disks.

## Read first
- `docs/linux-write-contract.md`, `docs/linux-file-guards.md`, `docs/linux-recovery.md`
- `src-tauri/src/linux_guard/`, `src-tauri/src/linux_journal/`, `src-tauri/src/linux_files/`, `src-tauri/src/platform.rs`

## Steps
1. Detect the file system from `statfs` and `/proc/self/mountinfo` (fuse subtype, `ro`, case folding) and return its tier with a reason. Replace the ext4 gate by the tier table. Check: unit tests on recorded mountinfo and statfs samples.
2. Capability checks and fallbacks for the reduced tier. Check: native tests per image (btrfs, xfs, f2fs if the tools exist, ntfs3, ntfs-3g, vfat, exfat, ext4 casefold).
3. Drill matrix per file system: save with BOM and CRLF, copy create and overwrite, external edit refusal, SIGKILL mid-save then recovery, undo, case collision. Check: one result table per file system in `docs/linux-write-contract.md`.
4. UI shows the tier and reason in the root probe message. Check: browser screenshot, both themes.

## Done when
- The drill matrix passes on every Full and Reduced image; refused file systems refuse with a clear reason.
- `docs/linux-write-contract.md` lists measured guarantees per file system.
- ext4 results are byte-identical to before.

## Gates
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory, never `/tmp`), `rustfmt --check`, clippy on touched files
- `bun run --bun check`, `bun test src/lib`

## Stop and report if
- A reduced-tier feature cannot meet the durability rule even with fsync.
- A drill needs root outside `udisksctl` loop setup.

## Report
Commit sha, files changed, each step's check result, the drill table, gate results, anything skipped.
