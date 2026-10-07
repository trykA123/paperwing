# Packet 29 backend handoff

Implemented steps 1 and 2, tracked discard in step 3, Linux untracked Trash, and tracked Recovery undo in step 5. Windows untracked discard refuses every file. Changes remain uncommitted.

## Blocking assumption

The existing Windows `trash::recycle` uses `SHFileOperationW` with `FOF_ALLOWUNDO` and `FOF_NOCONFIRMATION`. Microsoft documents undo preservation as best effort and provides a separate warning flag for permanent destruction. This does not establish the strict Recycle-Bin-only guarantee required by the brief. The new Windows untracked path therefore returns an unavailable reason and retains the file. The shared Windows recycler remains unchanged. No alternate deleter or recovery format was added.

Source: [Microsoft SHFILEOPSTRUCTW flags](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shfileopstructw).

The orchestrator must decide whether to harden the existing shared Windows recycler before enabling this path. Native Windows checks remain unavailable in this Linux environment.

## Steps and proof

| Step | Change | Regression proof |
| --- | --- | --- |
| 1 | Byte-preserving Myers diff, numbered hunks, inclusive line ranges, quoted paths and selected patches | 7 passing tests apply patches to fixture indexes and compare exact blobs and cached diffs. Coverage includes CRLF, missing final LF, adjacent replacements, renamed paths with spaces, partial adds/deletes, binary/invalid selections, and reconstruction of 4096 small sequence pairs. |
| 2 | `stage_hunks` and `unstage_hunks`; fixed `git -C` argv, stdin patches, cached apply/reverse, exit-code checks and fresh snapshot validation | 8 passing command tests cover committing one of two hunks, preserving the other working change, reverse staging, stale working bytes/index, unborn files, staged renames, line ranges, binary refusal, executable deletion modes and linked-file refusal. |
| 3 | Tracked replacement through Windows `files` or Linux `linux_files`; existing journal persists previous content before publication. Linux untracked files use the existing Trash metadata/move path with regular-file support. | 10 passing Rust tests across discard, Linux writer and Trash modules cover hunk splicing, renamed-file hashes, actual records/results, exact backups, missing storage, stale bytes/index, unchanged index, missing tracked files, Trash payload/info and unavailable Trash refusal. Windows untracked discard is blocked as described above. |
| 5 | Existing `recovery_undo` reads the normal discard records; record format and undo commands remain unchanged | Linux fixtures restore a whole file and a discarded hunk byte for byte. Further cases preserve later edits and restore a previous missing-file state. One Windows CRLF/path-with-spaces journal test was added but not executed. |

## IPC

Arguments below exclude the injected Tauri app handle. Details and selection semantics are in [partial-staging.md](partial-staging.md).

| Command | Arguments | Result |
| --- | --- | --- |
| `change_hunks` | `path, file, origPath, area` | `ChangeHunks` |
| `stage_hunks` | `path, request: HunkRequest` | void |
| `unstage_hunks` | `path, request: HunkRequest` | void |
| `discard_files` | `path, files: DiscardFile[], confirmed` | `DiscardOutcome[]` |
| `discard_hunk` | `path, request: HunkRequest, confirmed` | `DiscardOutcome` |

`HunkRequest = { file, origPath, area, contentHash, hunks }`.
`DiscardFile = { file, contentHash, origPath? }`.
`HunkSelection = { hunk, ranges: [{ start, end }] | null }`; indices are zero based and range ends are inclusive.
The wrappers and types are appended as `partialStagingApi` in `src/lib/api.ts`.

## Checks

The final native test environment used absolute ext4 paths inside this worktree:

```text
SKEIN_TEST_TMP=/mnt/Sabrent/homelab/.crew/paperwing-api-builder-p29-stage/src-tauri/target/p29-final-test-tmp
TMPDIR=/mnt/Sabrent/homelab/.crew/paperwing-api-builder-p29-stage/src-tauri/target/p29-final-test-tmp
CARGO_TARGET_DIR=/mnt/Sabrent/homelab/.crew/paperwing-api-builder-p29-stage/src-tauri/target
```

| Command/check | Result |
| --- | --- |
| `cargo test --offline --no-fail-fast --manifest-path src-tauri/Cargo.toml -- --test-threads=1` | 624 passed, 3 failed, 7 ignored. All 25 packet tests passed. Main and doctest targets contain 0 tests and passed. |
| `cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets --message-format=json` | Exit 0. Exact diagnostic comparison against isolated HEAD: 30 existing warnings, 0 new warnings. |
| `rustfmt --edition 2021 --check` on all 15 new Rust files, including child tests | Exit 0. |
| `bun test src/lib` | 400 passed, 0 failed, 2344 assertions across 42 files. Includes 2 new IPC wrapper tests. |
| `bun run check` with isolated writable dependency cache | 294 files, 0 errors, 0 warnings. Original `node_modules` symlink restored. |
| `bun run --bun check` | Failed in esbuild preprocessing; normal Bun script invocation above passed. |
| `git -c core.whitespace=cr-at-eol diff --check` | Exit 0. |

The remaining native failures reproduce on isolated HEAD:

- `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
- `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: ACL setup returns `EINVAL`.
- `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: ACL setup returns `EINVAL`.

Earlier default-parallel runs encountered an executable fixture setup error, a concurrent rebuild replacing a process-test binary, reused-PID collisions in retained legacy fixtures, and existing shared-probe flakiness. The fixture setup was corrected. The final serial run preserved old evidence, used fresh directories and had only the three verified environment failures above.

## Files and retained artifacts

New Rust files: `commit/{patch,snapshot,stage,discard,test_fixture}.rs`, `commit/patch/{lines,tests}.rs`, `commit/{stage,discard}/tests.rs`, `files/discard.rs`, `linux_files/discard.rs` and its tests, `linux_guard/folders/trash_file.rs`, `trash/{untracked,linux_file}.rs`.

Small wiring edits: `commit.rs`, `files.rs`, `lib.rs`, `linux_files/{commands,mod}.rs`, `linux_guard/folders.rs`, `trash.rs`, `trash/linux.rs`. Also appended `src/lib/api.ts`, added its wrapper test, updated `CHANGELOG.md` and added these backend docs.

Gate logs are under `src-tauri/target/p29-*.log` and the Clippy JSON logs beside them. An isolated HEAD source copy is retained under `src-tauri/target/p29-reference/node_modules/head-baseline`.

Earlier legacy fixtures are preserved under `src-tauri/target/p29-preserved-evidence/{12-native,13-native,14-native}`. Their original paths are `.skillify/evidence/paperwing/{12/native,13/repair-1/native,14/resume/native}`. To restore earlier evidence, move the fresh corresponding directory aside, then move its preserved directory back. No evidence was deleted.

Skipped: step 4/UI, frontend build/browser/CSS gates, live Tauri discard IPC, native Windows execution and manual Windows Recovery checks. Commit semantics, compare-copy and save paths remain unchanged. No project Git writes, commit, merge, push, deploy or server startup occurred.
