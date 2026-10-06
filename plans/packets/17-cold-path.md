# 17 — Cold path, phase 1: batched Git and in-process work

Status: in progress (uncommitted Codex run in `.crew/paperwing-api-builder-sfzxd`, third run)
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
- `cd src-tauri && cargo test --offline` (set `PAPERWING_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy
- Windows VM: serial `cargo test --locked`

## Stop and report if
- Any final result, count or limit differs from the oracle.
- Cancellation cannot interrupt a batch read.
- No bottleneck is attributable, or the Windows numbers show no gain.

## Report
Commit sha, files changed, the before/after table (Linux and Windows), gate results, anything skipped.
