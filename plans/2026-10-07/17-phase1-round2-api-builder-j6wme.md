# Packet 17 phase 1 review round 2

Implemented the six decided fixes on `crew/api-builder-j6wme`, based on `9eeecd5f1d0023fa2f0689aac710ca209c799f60`. Project HEAD is unchanged; changes remain uncommitted. Read the complete packet at HEAD and on main, including round 2, and the previous 368uj report before editing.

`git merge main` was skipped: the brief requires it, but the later run rules explicitly prohibit merging. The clarification remained unanswered. These gates cover this branch, without the latest main integration. No project commit, push, merge, deployment or application server was started.

## Files

- `src-tauri/src/compare.rs`, `compare/{count_eligibility,line_counts,text_diff}.rs`.
- `src-tauri/src/git.rs`, `git/{batch,batch_repository,runner}.rs`, `src-tauri/src/lib.rs`.
- `src-tauri/src/compare/tests.rs`, `compare/tests/{count_config,review_fixes,cold_measure}.rs`, new `compare/tests/review_round2.rs`.
- `docs/testing.md`, `CHANGELOG.md`, this report.

Existing line-ending kinds are preserved, including mixed endings in `git.rs`. No whole-file formatting of the existing runner or comparison service occurred. `src-tauri/src/diagnostics/`, `files.rs`, `file_guard.rs`, and the Linux implementation modules were untouched.

## Per-fix change and failing-first proof

All named new controls below live in `compare/tests/review_round2.rs`. Nine controls failed against the original implementation. The child-directory control failed against the first eligibility revision, then passed after the conservative guard. Final comparison suite: 81 passed.

| Fix | Change | Regression control |
| --- | --- | --- |
| 1 | Pass `--diff-algorithm=<value>` to tree and bounded diff metadata. Keep `-c diff.renameLimit`. | `tree_and_bounded_metadata_honor_the_selected_diff_algorithm`: Myers yields 5/5 and histogram 6/6; checks both paths with renameLimit 50000. Original histogram yielded 5/5. |
| 2 | Cache one check-attr probe for materialization names, using the legacy `core.attributesFile=` override. Resolve source metadata when storage has no enclosing repository. LF counts can run without an environment opt-in. CR-bearing inputs use Git. Storage ancestor attributes and unsafe storage paths keep the conservative fallback. | `unrelated_source_attributes_do_not_disable_storage_counts`; `autocrlf_storage_counts_match_git_with_and_without_normalized_eol`; `attributes_for_materialized_child_paths_keep_the_git_fallback`. The original rejected eligible contexts and returned 1/1 instead of Git's 0/0 under automatic EOL conversion. The child-pattern revision returned 1/1 instead of unavailable counts before its guard. |
| 3 | Share a reader for endpoints with the same Git directory; cap independent readers at 12. Runner capacity stays separate. | `same_repository_endpoints_share_the_reader_lifecycle`; `twelve_readers_leave_runner_capacity_available`. Original sides closed independently, and readers 7–12 failed. The thirteenth reader still refuses; metadata commands work at capacity. |
| 4 | Drain and cancel sessions synchronously under the session mutex; return a future containing only reader closes. Page-load callback creates that future before spawning it. | `release_drains_old_sessions_before_its_future_is_polled`. Original retained old sessions until polling and removed a newly opened session. |
| 5 | Reset attribute eligibility and cached diff configuration on open and refresh. | `opening_a_comparison_invalidates_storage_attribute_eligibility`; `opening_a_comparison_reloads_the_diff_configuration`. Original open retained obsolete eligibility/configuration. |
| 6 | Revoke readers in the shared runner before fetch/pull, covering comparison fetch and clone job updates. Parse Git options without treating config values as commands. | `fetch_and_pull_revoke_readers_before_the_git_operation`: local Git fetch/pull with live readers; unrelated config values retain readers. Original left readers live. |

The existing controls were updated for applicability, the 12-reader cap and CR fallback. The system-attribute fixture delegates to real installed Git with an injected attribute file; it emulates system lookup and does not replace native Git for Windows acceptance.

## Autocrlf finding

Git 2.56.0 no-index diff converts CRLF under both `core.autocrlf=true` and `input`, even when `check-attr -a` is empty. CRLF versus LF returns 0/0; with false it returns 1/1. A repository-local false override does not mask global conversion in unrelated materialization storage. Therefore attribute emptiness alone cannot authorize raw Rust counts for CR-bearing content. The matrix covers false/true/input, EOL normalization on/off, BOM, mixed/lone CR, incomplete final lines and LF edits. Source and configured global attributes do not apply when the legacy diff overrides `core.attributesFile=`.

## Commands and gates

All final native temporary roots were absolute ext4 paths inside this worktree, without `..`. `SKEIN_TEST_TMP` used `/mnt/Sabrent/homelab/.crew/paperwing-api-builder-j6wme/src-tauri/target/packet17-round2/full-gate-tmp`; process fixtures used `/mnt/Sabrent/homelab/.crew/paperwing-api-builder-j6wme/src-tauri/target/packet17-round2/gate-process-evidence` with their ownership marker. Comparison-only runs used separate fresh roots in the same evidence directory.

| Command | Result |
| --- | --- |
| `bun run --bun check` | Exit 1: 3 errors, 0 warnings, in unchanged BrandMark/WindowControls/Tabs styles; esbuild service stopped. Same failure reported previously. |
| `bun test src/lib` | Exit 0: 201 passed, 0 failed; 1,839 assertions in 22 files. |
| `bun run --bun build` | Exit 1: esbuild service stopped while loading Vite configuration. |
| `SKEIN_TEST_TMP=... SKEIN_PROCESS_EVIDENCE=... cargo test --offline --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1` | Exit 101: 530 passed, 3 failed, 7 ignored; 540 library tests. |
| `SKEIN_TEST_TMP=... cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib compare::tests -- --test-threads=1` | Exit 0: 81 passed, 0 failed. |
| `rustfmt --edition 2021 --check <18 Rust files>` | Exit 0. Includes all 17 phase 1 additions plus review_round2; list in evidence. |
| `cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets` | Exit 0. No new warning messages: 19 library and 10 test warnings, including 6 duplicates, equal to the frozen baseline. |
| `cargo test --release --offline --locked --manifest-path src-tauri/Cargo.toml --features benchmark --lib --no-run` | Exit 0 before fixes and after final edits. Both executables frozen and hashed. |
| `python3 src-tauri/target/packet17-round2/measure.py` | Final resumed run exit 0: 150 successful samples, all fingerprints match per workload/options. |
| `git -c core.whitespace=cr-at-eol diff --check` | Exit 0. |

The three full-suite failures also failed individually on the frozen original executable:

- `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
- `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: EINVAL from the ACL operation.
- `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: EINVAL from the ACL operation.

The full Cargo invocation stopped in the library suite; binary/doc harnesses did not run. An exploratory frozen release full-suite run aborted in a process fixture and is not a passing gate. The final gate used owned ext4 process evidence and completed all library tests. An earlier comparison run during concurrent builds hit two subprocess ENOENT errors; the isolated rerun passed. Those exploratory logs are retained separately from final results.

## Release benchmark

Same cold_measure harness, fixture generator and process-tree RSS sampler as the previous report. Monorepo: 1,500 files; flat fixture: 20,000; each changes 100 files. Five fresh processes per workload and engine, with alternating order. True mode compares the legacy oracle, the frozen original round 1 executable, and round 2. False mode compares legacy and new in the candidate executable because the frozen original harness hard-codes true. Result fingerprints exclude generated file IDs only and include the actual options.

Values are median milliseconds / Git starts / sampled peak RSS MiB. App caches/processes are fresh; OS caches are uncontrolled. Timing and RSS cover backend preparation between markers, excluding WebView rendering, result serialization and shutdown.

`normalize_eol=true`:

| Fixture / workload | Legacy: ms / Git starts / RSS MiB | Round 1: ms / Git starts / RSS MiB | Round 2: ms / Git starts / RSS MiB |
| --- | --- | --- | --- |
| skein-fixture-mono / refs | 2787.58 / 319 / 85.23 | 55.37 / 31 / 56.92 | 48.91 / 24 / 50.36 |
| skein-fixture-mono / cross | 7432.82 / 3215 / 85.60 | 185.62 / 35 / 76.79 | 165.85 / 23 / 54.71 |
| skein-fixture-mono / working | 16152.67 / 322 / 85.43 | 130.13 / 30 / 49.30 | 134.35 / 24 / 53.85 |
| 20000-files / refs | 2625.17 / 319 / 127.05 | 96.43 / 31 / 92.55 | 86.07 / 24 / 85.86 |
| 20000-files / cross | 34698.78 / 40215 / 151.65 | 1502.80 / 35 / 94.48 | 1519.04 / 23 / 93.05 |
| 20000-files / working | 7190.45 / 377 / 153.36 | 657.73 / 48 / 92.03 | 652.48 / 42 / 98.89 |

`normalize_eol=false`:

| Fixture / workload | Legacy: ms / Git starts / RSS MiB | Round 2: ms / Git starts / RSS MiB |
| --- | --- | --- |
| skein-fixture-mono / refs | 158.86 / 219 / 84.67 | 44.28 / 24 / 79.04 |
| skein-fixture-mono / cross | 4348.61 / 3115 / 86.82 | 162.97 / 23 / 55.77 |
| skein-fixture-mono / working | 2685.24 / 222 / 86.46 | 131.32 / 24 / 52.70 |
| 20000-files / refs | 196.74 / 219 / 127.21 | 83.90 / 24 / 83.12 |
| 20000-files / cross | 29299.02 / 40115 / 151.23 | 1468.47 / 23 / 93.49 |
| 20000-files / working | 4488.07 / 277 / 137.18 | 641.15 / 42 / 95.75 |

All legacy-versus-new medians improve, and Git starts fall in every workload and option mode. Against round 1, monorepo working is 3.24% slower (130.13 → 134.35 ms), and large cross is 1.08% slower (1502.80 → 1519.04 ms). Thus the stricter “no regression versus round 1” condition is not established. Median RSS also increases versus round 1 for both working workloads. Maximum sampled RSS is 165.97 MiB after versus 164.66 MiB in round 1; these samples do not establish p95 memory or cancellation behavior.

The first benchmark attempt retained 45 valid monorepo samples, then stopped because large fixture creation had failed its clone and no manifest existed. I had not checked that creation process's completion before measuring. Retried cloning inside this worktree, verified 20,000 tracked files and 100 changes, created the manifest, and resumed without replacing valid samples. The failed attempt is excluded from the 150 successful samples. One large legacy working run took 102.50 seconds; medians retain all successful samples.

## Open risks and evidence

Main integration remains unperformed because of the contradictory merge instruction. Full gates are not green due to the existing frontend service and native environment failures. Native Windows CI/Defender, WebView acceptance, temp-file/byte totals and cancellation p95 were not run. No Windows result is inferred from Linux tests. The two small regressions against round 1 and sampled RSS increases remain unresolved; no implementation change was made to manufacture better benchmark samples.

Evidence: `src-tauri/target/packet17-round2/`, including final logs, failing-first controls, warning/line-ending audits, source/binary hashes, fixture scripts/manifests, all 150 sample JSON/log files and both tables. No source code or diagnostics changes occurred during measurement. Mapify proposed, without persisting, the release-drain invariant at `compare/tests/review_round2.rs::release_drains_old_sessions_before_its_future_is_polled`; proposal is in `mapify-candidate.md`.

Final listener audit: `ss` could not open the netlink socket under the sandbox. `/proc/net/tcp` and `tcp6` showed no listeners in this execution namespace on ports 41020–41039.
