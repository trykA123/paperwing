# Partial staging and discard backend

Packet 29 implements backend steps 1–3 and Recovery undo for tracked discards.
The commit dialog and diff editor integration wait for packet 37.

The new IPC surface is exposed by `partialStagingApi` at the end of `src/lib/api.ts`:

| Command | Arguments | Result |
| --- | --- | --- |
| `change_hunks` | `path, file, origPath, area` | `ChangeHunks` |
| `stage_hunks` | `path, request: HunkRequest` | void |
| `unstage_hunks` | `path, request: HunkRequest` | void |
| `discard_files` | `path, files: DiscardFile[], confirmed` | `DiscardOutcome[]` |
| `discard_hunk` | `path, request: HunkRequest, confirmed` | `DiscardOutcome` |

`ChangeHunks` supplies a `contentHash`, `binary` and numbered `hunks`.
Hunk indices and line range bounds are zero based; range ends are inclusive.
A selection with `ranges: null` selects the whole hunk. Context lines do not change.
For a replacement, selecting its removed and added lines replaces the old text.
Selecting only removed or added lines removes or inserts those exact lines.
`text` excludes LF, retains CR, and `noNewline` marks a missing final LF.

`HunkRequest` contains `file, origPath, area, contentHash, hunks`.
Stage uses `unstaged` or `untracked`; unstage uses `staged`.
Discard hunk accepts one whole `unstaged` hunk.
`DiscardFile` contains `file, contentHash` and optional `origPath`, from a working-tree diff.
Pass the same `origPath` used to read a renamed file's diff.
Re-read the diff after each mutation. Changed working bytes, HEAD or this path's index entry refuse stale actions.
Unrelated index entries and index stat refreshes do not invalidate the content hash.
Binary content has no selectable hunks, but whole tracked files support recoverable discard.
Partial staging refuses overly complex diffs; whole-file staging remains available.

Working content passes through Git's clean conversion before hunk listing or staging,
so `core.autocrlf` and text/eol attributes match normal Git staging.
Selected patches use three context lines and run through `git apply --cached`, with
`--reverse` for unstage. Existing commit commands and whole-file staging keep their semantics.
Partial unstage retains a staged rename; selecting every change also reverses the rename.
Selections that place a missing-newline line before another line are refused with
"Select the last line's change too: the file has no final newline".
Files with an active `filter` attribute, including Git LFS, refuse hunk and discard actions.
Neither staging command changes working-tree bytes.

Tracked discard restores Git's smudged worktree form through Windows `files` or Linux `linux_files`.
This preserves the configured checkout line endings. Hunk discard smudges the selected result too.
Their existing journal persists verified before-bytes before replacing the file.
`recoveryId` identifies the normal record; existing `recovery_undo` restores it and refuses
later conflicting edits. The recovery format is unchanged.
An existing file without an index version is refused unless Git identifies it as untracked.

Intent-to-add entries (`git add -N`) use the untracked discard path and never restore an empty blob.
Linux untracked discard uses the same Trash service as Delete set, with file support
added to its move path. It returns `state: 'trashed'` and no Recovery record.
Restore those files from desktop Trash. Unavailable Trash refuses the move and retains the file.
Windows untracked discard currently refuses every file. The existing shared recycler uses
`SHFileOperationW` with best-effort `FOF_ALLOWUNDO`, which does not establish the requested
strict guarantee against permanent deletion. This is a backend limitation, not a recovery-format change.
The orchestrator must decide whether to harden the shared Windows recycler before enabling this path.
Batch discard reports each file independently; completed files remain recoverable if a later file fails.
`confirmed` must follow a confirmation naming every selected file or showing the selected hunk.

Linux writes retain the practical race limits documented in [the write contract](linux-write-contract.md).
Windows tracked discard and undo execution require the Windows VM.
