# 41 — Anonymous diagnostics export (test builds only)

Status: done except the Windows proof (windows-diag CI job runs on the next push)
Platform: Windows first; Linux parity so it can be tested on the dev box
Size: M
Role: both (backend first: api-builder; then Settings section: ui-builder)

## Goal
The owner installs a test build at work, uses Skein normally, then clicks "Generate diagnostics" in Settings. Skein writes one anonymous JSON file. It shows where time, Git process starts, memory and CPU go on Windows. It contains no repo, org, host, user, branch or file names, and no paths.

## Already done
- `src-tauri/src/benchmark.rs`: `Recorder` with per-phase spans (`git.queue`, `git.process`, `compare.*`, `ipc.*`, `ui.*`, `editor.*`), Git command counts by operation, 16384-event ring buffer, `benchmark_snapshot()`. Strings are `&'static str` from fixed lists. Spans and counts are active only with the `benchmark` Cargo feature.
- `src/lib/benchmark.ts`: frontend `ui.*`/`editor.*` timers, gated on `VITE_SKEIN_BENCHMARK=1`, sent through `benchmark_record`.
- `src-tauri/Cargo.toml` features: `benchmark`, `test-profile = ["benchmark"]`.
- `.github/workflows/build.yml`: `windows` job builds the NSIS installer as artifact `skein-windows-nsis`.
- `windows-sys =0.61.2` is already a Windows dependency.

## Decisions
- Cargo feature `diagnostics = ["benchmark"]`. Default and release builds do not contain the module, its commands or its UI. CI builds a separate artifact `skein-windows-diag-nsis`.
- Output is one file: `skein-diagnostics-<YYYYMMDD-HHMMSS>.json`, schema `version: 1`. No zip, no new archive dependency.
- Allowlist, not redaction. Every string in the output is either a `&'static str` from a fixed list in code, or a value from a closed enum. No free text from Git, the OS, settings or errors may reach the output.
- Repos are numbered `repo-1..n` in a random order per export. No hashes of names.
- Sizes and counts are rounded to 2 significant figures (1534 → 1500, 42.7 MB → 43 MB). Durations, memory and CPU are not rounded.
- Leak check before writing: build a denylist at runtime: OS username, computer name, every path component of the home dir and of each repo root, set names, repo folder names, remote URL host/owner/repo parts, configured GitHub/GHES hostnames and account logins. Keep tokens of 3+ characters. Search the serialized JSON case-insensitively for each token. Any hit refuses the export with "Diagnostics contained identifying text; nothing was written". The error does not name the token.
- Running Git for `scale` through the public runner with `-C <repo path>` is fine. The runner's activity list is the local Activity panel, which already shows paths for every feature. The export must never read `activity_snapshot()`, stderr or error text.
- Export only on click. No upload, no network, no background writing to disk. The sampler keeps data in memory only.
- No new crates. Windows memory, CPU and processes use `windows-sys` (add only the needed feature flags: `Win32_System_ProcessStatus`, `Win32_System_Threading`, `Win32_System_Diagnostics_ToolHelp`, `Win32_System_SystemInformation`). Linux reads `/proc`.

## Output content
- `machine`: os family, Windows build number, CPU logical cores, total RAM (rounded), system drive type (`ssd` | `hdd` | `unknown`), Git version (numbers only), Skein version, uptime of this session.
- `timings`: the `aggregates` and `commands` of `benchmark_snapshot()`, plus the events ring (phase, operation, duration, plus a `t` offset in ms from session start if cheap to add without editing `benchmark.rs`; otherwise omit `t`).
- `resources`: samples every 1 s, ring of 3600 (one hour). Each sample: `t`, Skein process working set and private bytes, CPU % since last sample, handle count (Windows), thread count; WebView2 total (all descendant `msedgewebview2.exe`: count, working set sum, CPU %); Git (descendant `git.exe` and its children: count, working set sum). Plus `peaks` for each of these.
- `scale`: computed at export time with progress, cancellable: number of sets, repos per set, and per anonymous repo: tracked file count, ref count, loose plus packed object size, worktree flag. Uses the existing Git runner with fixed args (`ls-files -z` counted while streaming, `for-each-ref --format=x` counted, `count-objects -v` parsed to numbers). The repo index is the only identifier.

## Scope
- Do: new module `src-tauri/src/diagnostics/` (split per `code-quality.md`: `mod.rs` commands, `sampler.rs`, `process_windows.rs`, `process_linux.rs`, `scale.rs`, `leak.rs`, `round.rs`, tests). Register commands in `lib.rs` under `#[cfg(feature = "diagnostics")]`. Cargo feature. CI job. Settings section. `docs/testing.md` section "Windows diagnostics".
- Do not: send anything over the network; log Git stderr, error messages or paths; add a crate; enable diagnostics in default builds.

## Read first
1. `src-tauri/src/benchmark.rs`
2. `src-tauri/src/lib.rs` (setup and `invoke_handler`)
3. `src-tauri/src/git/runner.rs` (public execute API only)
4. `src/lib/benchmark.ts`, `src/components/Settings.svelte`
5. `.github/workflows/build.yml`
6. `~/.agents/rules/code-quality.md`, `rust.md`, `typescript.md`, `web-ui.md`

## Do not touch
- `src-tauri/src/benchmark.rs`, `src-tauri/src/compare*`, `src-tauri/src/git/runner.rs`, `src-tauri/src/git.rs`: packet 17 (crew sfzxd) rewrites them. Read their public API only.
- `src/lib/api.ts` beyond adding new exported functions at the end.

## Steps
1. Cargo feature `diagnostics` and an empty module wired in `lib.rs`. Check: `cargo check --features diagnostics` and `cargo check` both pass; `cargo tree` unchanged apart from windows-sys features.
2. `round.rs` (2 significant figures) and `leak.rs` (denylist build plus scan). Check: unit tests cover a planted username, set name, URL owner and path component, each case-insensitive, and confirm the export refuses.
3. Process sampling (Windows plus Linux) and the 1 s sampler started in `setup` under the feature flag. Check: a Linux test spawns a child `sleep` and sees git/child counts change; the Windows code compiles in CI (`cargo check --target x86_64-pc-windows-msvc` if the toolchain exists locally; otherwise CI).
4. `scale.rs` with cancellable progress events. Check: a test on a fixture repo returns rounded counts and no strings except the repo index.
5. `diagnostics_status`, `diagnostics_preview` (returns the JSON text) and `diagnostics_export` (writes to a path chosen through the existing dialog plugin) commands. Check: an end-to-end test builds a document from fixture repos named `acme-secret-repo` under a set `ClientCorp` and asserts that neither string appears. A schema test asserts every string value belongs to the known lists.
6. CI: `windows-diag` job, `bun run --bun tauri build --features diagnostics` with `VITE_SKEIN_BENCHMARK=1`, artifact `skein-windows-diag-nsis`. Check: YAML valid; job mirrors `windows` with actions pinned the same way.
7. ui-builder: Settings section "Diagnostics", shown only when `diagnostics_status` succeeds. It shows a sampling indicator, a "Generate diagnostics" button (scale progress, then preview in a scrollable monospace block with byte size), and a Save button. Use existing tokens and components. Check: screenshots at 1440 and 1100 px, both themes, and section absent in a default build.

## Done when
- A default build has no diagnostics commands or UI (`cargo check` without the feature; frontend hides the section).
- A diagnostics build on Windows writes one JSON file with machine, timings, resources (including WebView2 and git.exe) and scale.
- The leak test with planted names passes; a planted name in any field refuses the export.
- `docs/testing.md` tells the owner how to install the diag build, use it, export and send the file privately (not to the public repo).

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (`--write` if the Settings CSS changes)
- `cd src-tauri && cargo test --offline` and `cargo test --offline --features diagnostics` (set `SKEIN_TEST_TMP` to an ext4 directory, never `/tmp`)
- `rustfmt --check` and clippy on touched Rust files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- Measuring WebView2 or Git children needs a new crate or admin rights.
- Getting `t` offsets or any other field needs an edit to `benchmark.rs` or the runner.
- The `benchmark` feature turns on behaviour beyond recording that would change a normal user session.

## Report
Commit sha, files changed, each step's check result, gate results, a sample export from Linux (attach the file path), anything skipped.
