# 17 — Cold path, phase 1: batched Git and in-process work

Status: phase 1 built (uncommitted in `.crew/paperwing-api-builder-sfzxd`); review says fix first, see "Phase 1 review fixes"
Platform: Windows first (Defender on), Linux parity
Size: L
Role: api-builder (gpt-6.1-sol xhigh), one writer
Phase 2 (gitoxide) is a separate packet: `plans/packets/17b-gitoxide.md`.

## Goal
A comparison with empty caches starts faster because it spawns far fewer Git processes and writes no temp files. Defender scans every process start and every new file, so those costs multiply on Windows. Final results stay byte-identical to today.

## Already done
Uncommitted in the Codex worktree (16 files changed, +1591/-671, plus new files). Read it with `git -C <worktree> diff --stat` and `git status`.
- `src-tauri/src/git/batch.rs` (new): `BatchReader`, one long-lived `git cat-file --batch` per endpoint, owned by the job, released on close, cancel, refresh and owner drop. Uses 30 of the 32 runner slots at most (`RESERVED_SLOTS`). Runs with `core.fsmonitor=false`.
- `src-tauri/src/compare/line_counts.rs` (new): `count`, an in-process line-count that replaces the per-file temp file plus `git diff --numstat`. It falls back to `None` (Git path in `text_diff.rs`) above 8192 lines or when lines repeat.
- `src-tauri/src/compare/working_inventory.rs` (new): hashes raw working-tree bytes in-process in the endpoint's object format, without Git filters. `object_id.rs` has `ObjectFormat` (SHA-1 and SHA-256, using `sha1` and `sha2` pinned in `Cargo.toml`). `index_objects.rs` validates index objects.
- Binary content IPC: `comparison_content` returns `tauri::ipc::Response`; `src/lib/api.ts` returns `Uint8Array` through the new `src/lib/content-bytes.ts`; `FileCompare.svelte`, `FilePreview.svelte` and `compare-state.svelte.ts` pass the entry kind.
- `src-tauri/src/benchmark.rs`: per-operation Git counters.
- Tests: `compare/tests/{cold_path,cold_lifecycle,cold_measure,equivalence,legacy,metadata_limits}.rs`, a frozen oracle in `compare/tests/legacy/`, and `src/lib/content-bytes.test.js`. `docs/testing.md` documents the controls and the ignored `measure_cold_comparison` harness (`PAPERWING_COLD_ROOT`, `_ENGINE=old|new`, `_WORKLOAD=refs|cross|working`, `_RESULT`).
- Prior controls are in `.crew/paperwing-api-builder-rzs29`.
- Not in the diff yet: streamed `diff-tree` rows, a single `ls-tree` for identical files, per-command status config.

## Decisions
- Results are byte-identical to today: rows, summaries, options, history, limits. Generated file ids are excluded from comparison.
- Readers are per repository. One Git object database cannot serve SHA-1 and SHA-256 reads together, so each endpoint owns its reader.
- Fast path only within one repository. Cross-repository comparisons keep the current path.
- Raw equality uses blob ids and raw hashes. Normalised equality (EOL, whitespace) never trusts raw Git statuses or counts; fall back per file when options change normalisation.
- "Counts unavailable" is never shown as "Same". No automatic network or mutation.
- Do not enable `core.fsmonitor`: the code disables it on purpose because fsmonitor runs hooks. `core.untrackedCache`, `core.preloadIndex` and `core.fscache` may be set per command for working-tree status only, if measurement shows a gain. Never change global config.
- Optional `--filter=blob:none` clones are a later, separate decision.

## Scope
- Do: finish and land the batch reader, in-process counts and hashing, binary IPC; then the batched tree reads below; the before/after table.
- Do not: caching (21), progressive delivery (18–20), gitoxide (17b), changing what a comparison means, raising the 32-slot runner ceiling.

## Read first
- `docs/testing.md` in the Codex worktree (cold-path controls)
- `src-tauri/src/compare.rs` (`Prepared`, `prepare`, `refresh`), `src-tauri/src/compare/inventory.rs`, `text_diff.rs`
- `src-tauri/src/git/runner.rs`, `src-tauri/src/git/batch.rs`
- `src/lib/api.ts`, `src/lib/content-bytes.ts`
- `plans/2026-10-06/handoff.md` (decisions in the brief)

## Do not touch
- `files.rs`, `file_guard.rs` (06), `src-tauri/src/linux_*`.

## Steps
1. Review the Codex diff by hand against the legacy oracle. Check: `cargo test --offline --locked --lib compare::tests -- --test-threads=1`, `bun test src/lib/content-bytes.test.js`, and the frontend gates run outside the Codex sandbox (Bun fails inside it).
2. Measure old versus new on Linux with the `cold_measure` harness on `skein-fixture-mono` and a 20,000-file fixture: wall time, process spawns, temp files, bytes, RSS. Check: one table in the commit message.
3. Add the batched tree reads: one `rev-parse`, one `diff-tree -r -z --raw -M` streamed as rows, one `diff-tree --numstat` for counts, `ls-tree -r -z` so identical files are marked by blob-id equality and never read. Check: equivalence fixtures (renames, type changes, binary, BOM and CRLF, filters, links, submodules, empty trees, unborn HEAD) match the oracle.
4. Re-measure on the Windows VM with Defender on, release build. Check: spawns and temp files per comparison drop; cancellation p95 and memory do not regress.
5. Commit with the table and merge after a `reviewer` pass.

## Done when
- Spawn and temp-file counts per comparison fall on Windows; the table shows it.
- All equivalence, lifecycle (reader release on close, cancel, refresh, drop) and SHA-256 fixtures pass.
- `comparison_content` sends binary with no `number[]` on the wire.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy
- Windows VM: serial `cargo test --locked`

## Stop and report if
- Any final result, count or limit differs from the oracle.
- Cancellation cannot interrupt a batch read.
- No bottleneck is attributable, or the Windows numbers show no gain.

## Report
Commit sha, files changed, the before/after table (Linux and Windows), gate results, anything skipped.

## Phase 1 review fixes (decided 2026-10-06 23:20)
Reviewer verdict on sfzxd: fix first. Git starts across repos fell from 1062 to 32, so keep the design. Start from main after `merge/perf` lands. Carry over the sfzxd diff without its formatting churn: revert the whole-file rustfmt of `git/runner.rs` and the CRLF→LF change in `git.rs`. Rename the leftover `PAPERWING_COLD_*`, `.paperwing-disposable` and `paperwing-diff-` names to Skein names (see `docs/naming.md`).
1. In-process line counts never run in the app: `compare/line_counts.rs` requires `GIT_ATTR_NOSYSTEM=1` in the app's environment. Decide eligibility per storage root, cached: the counter is used only when no gitattributes can apply (no system, global or `core.attributesFile` attributes, no `.gitattributes` or `info/attributes` in the repository). Otherwise fall back to Git. Decide before writing any temp files. Results must equal `git diff --numstat`.
2. Batch readers must not hold runner slots. Give them their own cap (6 sessions), close a reader after 30 s idle, and restart it at most once after a read error. Linked worktrees (`.git` is a file) must work through `--git-dir`/common dir rather than falling back.
3. Close all readers for a root before trash, branch cleanup, worktree removal and any write that can rename or delete the root.
4. Normal close: close stdin, wait up to 500 ms, use `taskkill` only if the process is still alive. `release_sessions` closes readers concurrently and does not block the UI thread.
5. `diff-tree` must honour the user's `diff.algorithm` and `diff.renameLimit`. Read them once per root and pass them with `-c`. Add fixtures with a user config (`histogram`, a high `renameLimit`) on both the fast and the fallback path. The fingerprint suite also runs with whitespace ignored and with EOL normalisation off.
6. Working-tree speed: keep one `ReadCache` per worker, not per 128-path chunk. Set `[profile.dev.package.sha1] opt-level = 3`. Measure before and after with a release build.
7. Frontend: `compare-state.svelte.ts` must not default an unknown entry kind to `'file'`. Use the server's kind, or fetch the row.
Done when: all phase 1 tests plus the new tests pass; Linux release-build medians are no worse than before on every workload; Git starts are reported; and the Windows CI job is green. Windows VM measurements follow as a separate step.

## After phase 1 merges
Architecture audit items 2, 8, 9, 10, 12, 13 and 19 (`plans/2026-10-06/audits/architecture.md`) touch the runner and compare code. Also pin the Git locale in the runner env (`LC_ALL=C`, `LANGUAGE=`) for stable messages. Write them as packet 43 once phase 1 is on main.

## Phase 1 review round 2 (2026-10-07 03:00)
Base 9eeecd5 on `crew/api-builder-368uj`. Reviewer: changes-needed.
1. `diff-tree` ignores `diff.algorithm` set with `-c` (it is plumbing). Pass `--diff-algorithm=<v>`; keep `-c diff.renameLimit`. Add a fixture where myers and histogram differ.
2. In-process counts never turn on with stock Git for Windows (`core.autocrlf=true`, the system `etc/gitattributes`). Decide eligibility with one cached `git check-attr -a -- left right` in the storage root. For autocrlf, measure real Git behaviour; in-process counts must equal the legacy Git result in every tested config.
3. The reader cap counts readers, not sessions. Share one reader when both sides use the same git dir, and raise the cap to 12 readers.
4. Race in the non-blocking release: drain sessions synchronously under a short lock, then spawn only the reader closes.
5. The eligibility cache is cleared only on refresh. Make config and attributes changes visible on open.
6. `close_root` before fetch and pull.
Then merge main, run the gates, and rerun the release benchmark (plus one run with `normalize_eol=false`).

## Windows CI round (2026-10-07, run 37579389858)
- Fixed on branch `fix/windows-verbatim-paths`: canonical Git directories carried the `\\?\` prefix and failed path validation, so batch readers never started on Windows.
- Still failing on `test-windows`:
  - CRLF line counts: `cold_path` cross_format and git_clean_crlf, and `review_round3` crlf_counts. Expected 2/2, got 1/1 or 0/0. Git for Windows ships `core.autocrlf=true` in its system config; the fixtures do not isolate it (`GIT_CONFIG_NOSYSTEM=1`).
  - Decide what raw counts mean under the owner's real config: raw compares bytes, so CRLF against LF lines are changes. Make the in-process path and the Git fallback agree, and isolate the test config.
  - `review_fixes::attribute_probes_match_no_index_configuration`: the test writes a Windows path into a global config file without escaping backslashes ("bad config line 2").
  - `cold_lifecycle` cancellation tests: one is the prefix error; the other times out (`Elapsed`). Re-check after the prefix fix.
- Linux `linux_guard::tests::bounded_private_files_and_handle_ownership_fail_closed` failed once with 7 against 6 handles. It looks flaky; make it robust or serialise it.
