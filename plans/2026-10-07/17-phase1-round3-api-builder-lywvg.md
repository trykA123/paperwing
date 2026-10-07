# Packet 17 phase 1, review round 3

Implemented the three fixes and the requested backlog entry on `crew/api-builder-lywvg`, based on `b8d5b4e`. Changes remain in this working tree. No project commit, merge, stash, push, deployment, or application server was started. Git writes were confined to disposable test and benchmark repositories created by the harnesses.

The changed production files are `src-tauri/src/compare.rs`, `compare/{count_eligibility,line_counts,text_diff}.rs`, and `git/runner.rs`. Controls are registered in `compare/tests.rs`; `compare/tests/review_round3.rs` is new. Existing controls in `compare/tests/{cold_lifecycle,count_config,review_fixes,review_round2}.rs` now use the cached EOL policy. `compare/tests/cold_measure.rs` adds `working-crlf`. Documentation changes are `docs/testing.md`, `plans/packets/backlog.md`, and this report.

`git/batch.rs`, `diagnostics/`, `stash/`, `commit.rs`, credentials, settings, and UI sources are unchanged. The runner's complete `Captured` structure and stderr helper implementation match the base byte-for-byte. There are no new uses of `raw_stderr` or `stderr_contains`.

## Fixes and regression evidence

| Finding | Change | Control and original result |
| --- | --- | --- |
| Open waits for fetch | Move diff configuration into a separate short-held standard mutex containing per-root Arc once-cells. Make eligibility reset synchronous under its own short lock. Reset only after the 16-session check succeeds. | `opening_a_comparison_does_not_wait_for_a_fetch`: original times out at 250 ms while the fetch seam is held. `rejected_open_keeps_the_configuration_cache`: original clears eligibility on the rejected seventeenth open. Both pass after the fix. Existing configuration reload controls also pass. |
| CR inputs always use Git | Cache effective `core.autocrlf` in the eligibility probe, querying storage's existing working directory without the source Git-directory override. `false` preserves CR bytes. `true` and `input` convert CRLF only without lone CR or NUL in the first 8000 bytes. Keep Git fallback for other inputs and ambiguous edits. | `crlf_counts_run_without_git_using_the_storage_autocrlf`: original Rust counts return None under false/true/input, rather than Git's expected counts. Fixed controls prove zero Git calls and no required materialization. `crlf_working_tree_fingerprints_match_legacy_for_every_autocrlf` compares full fingerprints with both EOL options and all three settings. A binary-boundary control covers bytes 7999/8000. |
| Unreadable cwd skips revocation | Inject cwd resolution through a narrow seam; use `unwrap_or_default()` so absolute `-C` still determines the fetch/pull root. | `absolute_fetch_and_pull_roots_survive_an_unreadable_cwd`: original returns None; fixed implementation resolves the absolute root despite an injected NotFound error. |
| Follow-up missing | Add the exact requested Compare revoke-and-restart line to `plans/packets/backlog.md`. | Verified in the working diff. |

The original debug binary and failing-first logs are retained under `src-tauri/target/packet17-round3/`. The two open controls, CRLF control across all three configurations, and cwd seam all failed before their production fixes. The full fingerprint control already matched the legacy oracle before the fixes and still matches afterward.

## Commands and results

Native checks use absolute ext4 roots inside this worktree, with `SKEIN_TEST_TMP` and `TMPDIR` set together. Final roots are `/mnt/Sabrent/homelab/.crew/paperwing-api-builder-lywvg/src-tauri/target/packet17-round3/full-final` and the marked `process-evidence-final` sibling. No native temporary root uses `/tmp` or `..`.

| Command | Result |
| --- | --- |
| `cd src-tauri && cargo test --offline` | Exit 101: 557 passed, 3 failed, 7 ignored, 567 library tests. All 87 comparison tests pass, as do all six new controls. The final production source run took 263.72 s. |
| Frozen original executable, each failing environment test with `--exact ... --nocapture --test-threads=1` | Each exits 101 with the same environment error. |
| `cd src-tauri && cargo test --offline --bin skein` | Exit 0: 0 tests. |
| `cd src-tauri && cargo test --offline --doc` | Exit 0: 0 tests. |
| Frozen final release executable, existing direct-counter control, with false/true/input isolated global configurations | Three successful runs, one test each. This separately validates the last existing-test adaptation. |
| `cd src-tauri && cargo clippy --offline --locked --all-targets` | Exit 0. Original and final both have 19 library and 10 test warnings, including 6 duplicates. Warning signatures and multiplicities are identical. |
| `rustfmt --edition 2021 --config skip_children=true --check <all 12 touched Rust files>` | Exit 1: pre-existing formatting in `compare.rs`, `compare/tests.rs`, `compare/text_diff.rs`, and `git/runner.rs`. The same four files fail at the base. Changed statements were formatted without whole-file churn. |
| `rustfmt --edition 2021 --config skip_children=true --check <eight touched leaf modules>` | Exit 0. Covers count eligibility, line counts, cold lifecycle, cold measure, count config, review fixes, review round 2, and review round 3. |
| `cargo test --release --offline --locked --manifest-path src-tauri/Cargo.toml --features benchmark --lib --no-run` | Exit 0 for original and final release harnesses. The original source was frozen in an isolated copy inside this worktree. |
| `git -c core.whitespace=cr-at-eol diff --check` | Exit 0. |

The three environment failures are unchanged:

- `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
- `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: ACL operation returns EINVAL.
- `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: ACL operation returns EINVAL.

The full Cargo command stops after its failed library suite. Binary and doc harnesses were subsequently run separately. The full-file formatting failure is pre-existing source formatting, not an environment failure. Strict Clippy is not claimed green; this run proves the requested absence of new warnings.

## Release benchmark

Pending completion. Final collection runs `python3 src-tauri/target/packet17-round3/measure.py` using the same ignored release cold-measure harness as round 2. Fixtures are regenerated inside this worktree at the same 1,500/20,000-file scales, with 100 changed files. The CRLF checkout has those 100 changed files in CRLF and local plus isolated global `core.autocrlf=true`. Exact historical fixture bytes were not available in this worktree, so the table compares original and fixed engines on identical regenerated fixtures rather than asserting direct timing parity with the earlier report.

Five fresh processes per engine/workload/options, alternating order. True mode compares legacy, original `b8d5b4e`, and fixed code. False mode compares legacy and fixed code, as in round 2. Fingerprints exclude generated file IDs only. App caches are fresh; OS caches are uncontrolled. Time and process-tree RSS cover backend preparation between markers, excluding WebView rendering, serialization, and shutdown. RSS is sampled every 2 ms; shared pages can be counted twice and brief peaks can be missed.

Exploratory samples are retained separately and excluded from the final table. They overlapped fixture preparation and exposed a sampler bug: libtest prefixes the start marker with the test name. Final collection recognizes that marker correctly and captures nonzero RSS.

Native Windows/Defender and Windows CI acceptance were not run. Fetch still permanently closes existing batch readers; the requested backlog line records the later revoke-and-restart work.
