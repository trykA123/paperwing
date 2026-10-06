# Linux folder workflows

Clone, fetch, pull, switch and reclone use the accepted [Linux guard contract](linux-file-guards.md).
They require a registered destination and saved origin/ref, a user-owned writable local ext4
workspace, confined parent resolution, and fresh root, ancestor and Git metadata identities.
Linux clone admission retains its lease through queued jobs, native moves and Git cleanup.
The trash command checks clone admission while holding the filesystem writer gate.

Clones run in a private sibling directory through a retained descriptor path. Only a successful
checkout publishes into the destination, using a same-mount no-overwrite rename. Failed or
cancelled clone data stays in its reported staging path. Recloning first verifies the existing
origin and preserves the checkout at a unique sibling `<folder>.bak-<random>` path. Neither
preserved checkouts nor failed staging directories are cleaned up automatically.

Folder recycling implements the [freedesktop Trash specification](https://specifications.freedesktop.org/trash/latest/)
directly with the existing pinned Rustix/libc primitives; no GIO dependency is added.
Folders on the home-data mount use `$XDG_DATA_HOME/Trash`, or `~/.local/share/Trash` by default.
Other mounts use a valid sticky, unlinked `$topdir/.Trash/$uid`, falling back to
`$topdir/.Trash-$uid`. Trash storage, `files` and `info` require current-user ownership and
mode0700. Linked, unsafe or unavailable storage refuses recycling.

Each unique item receives an exclusive mode0600 `.trashinfo` before its folder moves.
`Path` uses percent-encoded filesystem bytes: absolute for home Trash, mount-relative otherwise.
`DeletionDate` uses local time in `YYYY-MM-DDThh:mm:ss` form. The info file and its directory
are synchronized first; source and payload parents are synchronized after the rename.
Destination collisions never overwrite existing data. Cross-mount moves are refused;
there is no copy/delete or permanent-delete fallback. Failed moves retain any info artifact.
Errors after a move identify both candidate locations for inspection and keep configuration.

Only registered, unshared ordinary repositories qualify. Each folder returns its own
trashed, missing, skipped or failed result. A Linux skipped/failed recycle keeps the set;
the removal dialog offers **Remove configuration only**, which leaves every folder on disk.
Windows clone, Recycle Bin and partial-removal configuration behavior remain unchanged.

Restore a fixture by reading its `.trashinfo`, checking the expected original and payload
identities, then moving the payload back to an absent original destination on the same mount.
Preserved reclones restore the same way from their reported sibling paths. Native tests
exercise reverse renames and verify original inode, file bytes and modes.

This follows the approved practical race limits: descriptors retain objects and repeated
checks detect observed namespace substitutions, but a non-cooperating writer can still race
the final check. Ordered fsync and process-interruption evidence do not prove power-loss
durability. The desktop may empty Trash independently. No automatic restore or cleanup runs.
