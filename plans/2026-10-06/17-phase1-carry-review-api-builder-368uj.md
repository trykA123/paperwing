# Packet 17 phase 1: carry and review fixes

Ported the sfzxd carry onto main `ce4472b570da7a3b37cbbdae21f51da2330dd968`, on `crew/api-builder-368uj`. Applied review fixes 1–7. Left all changes uncommitted. The full gate remains blocked by failures reproduced on main and unrun Windows acceptance.

## Files

21 existing files changed and 19 source files added, plus this report:

- Backend: `src-tauri/{Cargo.toml,Cargo.lock}`, `src-tauri/src/{benchmark.rs,compare.rs,git.rs,lib.rs,branch_cleanup.rs,clone.rs,trash.rs}`, `clone/linux.rs`, `git/runner.rs`, `compare/{inventory.rs,text_diff.rs,tests.rs}`.
- New backend modules: `compare/{count_eligibility.rs,index_objects.rs,line_counts.rs,object_id.rs,working_inventory.rs}`, `git/{batch.rs,batch_repository.rs}`.
- New Rust controls: `compare/tests/{cold_lifecycle.rs,cold_measure.rs,cold_path.rs,count_config.rs,equivalence.rs,legacy.rs,metadata_limits.rs,review_fixes.rs}`, `compare/tests/legacy/{inventory.rs,text_diff.rs}`.
- Frontend: `src/lib/{api.ts,compare-state.svelte.ts,workspace.test.js,content-bytes.ts,content-bytes.test.js}`, `src/components/{FileCompare.svelte,FilePreview.svelte}`. Documentation: `CHANGELOG.md`, `docs/testing.md`.

Existing line-ending kinds match `git show HEAD:<path> | file -`, including mixed endings in `git.rs`. Removed whole-file formatting churn. New benchmark variables, markers and diff-directory names use Skein names. Historical identifiers listed in `docs/naming.md` remain intact. Prohibited files and concurrent packet 41 work were untouched.

## Review fixes and proof

1. Cache count eligibility for each storage/source context before materialization. Check system/global/configured attributes, tracked/untracked repository attributes, linked metadata and storage ancestors. Automatic EOL conversion also requires Git fallback because `core.autocrlf` acts as automatic text attributes. Controls cover every placement, refresh invalidation, zero Git/materialization for eligible counts, and oracle equality. The new EOL regression failed before this fallback and passed afterward, including a repository override masking global conversion.
2. Batch readers use six independent permits, never runner slots. Idle shutdown takes 30 seconds; read errors allow one restart. Resolve `.git` files and common directories with identity checks. Controls cover capacity, concurrent metadata commands, idle reopening, restart exhaustion and external linked metadata.
3. Revoke readers before Trash, branch deletion, worktree removal and reclone root moves. Trash and reclone repeat revocation under the filesystem write gate. Controls exercise actual branch deletion, relative linked-worktree removal and root rename after closing all readers.
4. Normal shutdown closes stdin and waits 500 ms. Windows taskkill runs only after timeout while the process remains alive. Session/reader closure uses concurrent joins; page reload schedules cleanup asynchronously. Lifecycle controls cover close, cancel, refresh, release, owner drop and cancellation during reads. Windows process behavior remains unrun.
5. Read `diff.algorithm` and `diff.renameLimit` once per root using an async once-cell, without holding the state mutex during Git. Pass explicit `-c` values to fast `diff-tree` and bounded fallback `diff`. Adversarial and long-path fixtures use histogram/high rename limits; fingerprints cover all four EOL/whitespace option combinations.
6. Four working-inventory workers retain one ReadCache each across their entire partition. SHA-1 development builds use opt-level 3. Preserve raw SHA-1/SHA-256 hashing without filters. Scaled/cancellation/oracle controls pass; the release table measures both corpora.
7. Content requests require the server row kind. Unknown rows trigger paginated lookup; absent sides refuse the request. Binary IPC returns exact bytes through Uint8Array. Bun controls cover fetched rows, symlinks/gitlinks, absent sides, byte classification and stale/invalid responses. Rust verifies a raw binary IPC body.

## Commands and results

Commands ran inside this worktree. Rust temporary roots used absolute ext4 paths under `.p17`, without `..`. Evidence was moved afterward to `src-tauri/target/packet17-evidence`; logs retain original paths.

| Command | Result |
| --- | --- |
| `bun run --bun check` | Exit 1: 3 errors, 0 warnings. Same esbuild failures on main. |
| `bun test src/lib` | Exit 0: 201 passed, 0 failed; 1,839 assertions across 22 files. |
| `bun run --bun build` | Exit 1: esbuild service stopped. Same failure on main. |
| `SKEIN_TEST_TMP=$PWD/.p17/test-tmp-gate-v3 cargo test --offline --locked --manifest-path src-tauri/Cargo.toml` | Exit 101: 520 passed, 3 failed, 7 ignored; 530 library tests. |
| `SKEIN_TEST_TMP=$PWD/.p17/test-tmp-compare-final cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib compare::tests -- --test-threads=1` | Exit 0: 71 passed, 0 failed. |
| `rustfmt --edition 2021 --check <17 created Rust files>` | Exit 0. Exact file list in `new-rust-files.json`. |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets` | Exit 0: no new warning messages/counts versus main; both report 19 library and 10 test warnings, including 6 duplicates. |
| `cargo test --release --offline --locked --manifest-path src-tauri/Cargo.toml --features benchmark --lib --no-run` | Exit 0: optimized release harness built for current main and candidate. |
| `python3 .p17/measure.py` | Exit 0: 60 successful samples; all six result fingerprints match before/after. |
| `git -c core.whitespace=cr-at-eol diff --check` | Exit 0. |

Three Rust failures also fail individually on the frozen main executable:

- `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
- `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: ACL operation returns EINVAL.
- `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: ACL operation returns EINVAL.

Frontend check errors remain in unchanged BrandMark, WindowControls and Tabs styles. The full Rust command stops in the library suite; downstream binary/doc harnesses did not run. Earlier fixture collisions from reused sandbox process IDs were discarded; the final suite uses a fresh test root and process-ID range.

## Linux release medians

Actual main versus candidate; five fresh process/cache runs per workload and engine, alternating order. Monorepo fixture has 1,500 files; the flat fixture has 20,000. Both change 100 files. All serialized rows, summaries, history and options match, excluding generated file IDs. OS caches were uncontrolled. Wall time includes backend preparation; RSS sums the process tree, sampled every 2 ms between measurement markers. It includes shared pages and excludes WebView rendering and shutdown.

| Fixture / workload | Main: ms / Git starts / peak RSS MiB | After: ms / Git starts / peak RSS MiB |
| --- | --- | --- |
| skein-fixture-mono / refs | 2532.68 / 319 / 83.86 | 49.84 / 31 / 58.94 |
| skein-fixture-mono / cross | 6749.46 / 3215 / 84.53 | 156.61 / 35 / 55.29 |
| skein-fixture-mono / working | 5033.29 / 322 / 84.71 | 128.47 / 30 / 54.60 |
| 20000-files / refs | 2620.06 / 319 / 123.83 | 98.49 / 31 / 91.89 |
| 20000-files / cross | 30697.54 / 40215 / 148.63 | 1502.16 / 35 / 90.42 |
| 20000-files / working | 6483.42 / 377 / 133.57 | 589.18 / 48 / 98.71 |

No workload is slower. All median sampled peak-RSS values are lower. Individual RSS peaks vary: candidate large-fixture refs/cross maxima were 166.50/167.62 MiB versus main 142.73/149.96 MiB. These spikes remain a memory risk; five samples do not establish p95 behavior.

## Skipped and open risks

Windows CI was not run; required green CI remains unverified. Windows VM/Defender measurements are a separate step. Native WebView IPC/browser checks were not run because frontend build fails. Temp-file/byte totals and cancellation p95 were not benchmarked; functional controls exercise no-materialization and cancellation behavior.

The existing bounded raw diff-tree capture/fallback remains; streamed rows and further tree/rev-parse batching were outside this carry-and-review-fixes brief. No design decisions beyond the packet were introduced. No server started. No commit, push, merge or deployment occurred.

Evidence includes command logs, the warning comparison, line-ending signatures, source hashes, patch/new files, fixture generators/manifests, all sample JSON/logs and frozen binary SHA-256 values. Discarded/incomplete exploratory measurements are separate from `results/`. Restore the original replay layout by copying this ignored evidence directory to `.p17` before running its scripts.
