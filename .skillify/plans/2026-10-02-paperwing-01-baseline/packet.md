# Packet 01 — baseline, isolated fixtures and characterization

**Status:** Proposed; requires user approval. **Weight:** Standard. **Depends on:** none.
**Read first:** `README.md`, `package.json`, `src/lib/workspace.test.js`, `src-tauri/src/compare.rs` tests, and the working plan's Common execution/verification rules.

## Outcome and scope
Establish reproducible behavior/performance evidence and a disposable native test profile before refactoring. In: test-loader/golden infrastructure, Windows test-only `rtk` launcher removal, deterministic local fixtures, opt-in release instrumentation and feature/host matrix. Out: fixing application behavior, remote repositories, cache/prewarm, source moves and dependency upgrades.

## Requirements and invariants
- R1: Existing test discovery/results and current native feature availability are recorded, with Linux gaps labeled as baseline failures.
- R2: New rune modules and shared layout vectors can be tested without coupling Rust to a JavaScript declaration's spelling.
- R3: Another operator can replay fixtures and collect first useful render/full completion and independent command counts in a release build.
- I1: No real settings, tokens, repositories or recovery data are used, overwritten or logged; instrumented release is opt-in.
- I2: Tests characterize existing final results/safety, including intentional quirks; no behavior fix is hidden here.

## Evidence
- [FACT] `workspace.test.js` registers a narrow rune compiler filter before dynamic imports; `compare.rs::tests` reads its layout declaration.
- [FACT] Activity retention is bounded; retained entries cannot count all subprocesses.
- [FACT] `paths.rs` Windows junction test calls `rtk bun -e`; `rtk` is not a documented prerequisite. This is a known test-harness issue to record then correct in P2, not an application behavior change.
- [ASSUMPTION] This Linux host and an owner-supplied Windows host can run native acceptance. P1 records actual availability; absence is not a pass.
- [DECISION] Performance targets/policies remain unratified until P5, not fixed obligations copied from the earlier proposal.

## Steps
- P1 [BATCH]: Record revision/status, hardware/filesystem/session, tools, installed dependencies, existing tests (including ignored counts), and baseline Linux/Windows feature matrix.
  - Depends on: none. Location: existing README check commands and Tauri config → baseline evidence under proposed ignored `.skillify/evidence/paperwing/01/`.
  - Verify: `bun --version`; `rustc --version`; `cargo --version`; `git --version`; `bun run --bun check`; `bun test src/lib/workspace.test.js src/lib/workspace.test.ts`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`; `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.
  - Fails if: a baseline defect is attributed to a later change or skipped Windows tests are counted as Linux coverage. Obtain approval before dependency downloads.
- P2 [BATCH]: Broaden the compiler to project `*.svelte.ts`, preserving server-test semantics and registration-before-import. Add a small rune-module test; move identical layout vectors to proposed `src/lib/test-support/layout-vectors.json`; make JS and Rust tests read that single JSON fixture. Replace the Windows test's `Command::new("rtk").args(["bun", "-e", script])` with direct `Command::new("bun").args(["-e", script])`, preserving its script, environment, positive control and safety assertions.
  - Depends on: P1. Location: `workspace.test.js::Bun.plugin`, `compare.rs::tests::custom_layout_matches_typescript_goldens_and_registered_destination`, Windows junction block in `paths.rs` tests, proposed `src/lib/test-support/svelte-loader.js`.
  - Verify: `bun test src/lib/workspace.test.js src/lib/workspace.test.ts`; full serial Cargo tests on Linux and Windows; compare counts/assertions/vectors to P1 and prove the Windows junction fixture executed successfully with `rtk` absent.
  - Trap: moving the JS declaration without updating its Rust parser, dropping a native junction assertion while fixing its launcher, or mistaking SSR tests for client-effect proof.
- P3 [BATCH]: Add characterization fixtures for final comparison option/result fingerprints, tree/metadata response races, tab/editor guards, whole-set two-worker teardown, copy/recovery limits and staged-index semantics. Preserve expected existing behavior, including bugs to change in later packets.
  - Depends on: P2. Location: existing frontend suites and inline Rust suites; proposed domain-specific tests listed explicitly by the harness.
  - Verify: `bun test src/lib`; full serial Cargo tests; native Windows safety tests with ignored-test accounting.
  - Fails if: mock-only checks claim filesystem safety or new assertions silently redefine semantics.
- P4 [ISOLATE]: Create deterministic disposable Git fixtures and an opt-in instrumented release/profile runner, separate from Activity. Pin Git author/config/EOL/object inputs; refuse any unmarked existing root. Verify resolved app data/cache/config/WebView and credential service/source-ID isolation before launch.
  - Depends on: P3. Location: proposed `scripts/testing/{fixtures,baseline,native-profile}.ts`, `src-tauri/tauri.test.conf.json`, proposed instrumentation module; narrow hooks in `git.rs::execute_inner`, comparison phases, `CompareState`, FileCompare initialization.
  - Verify: `bun run scripts/testing/fixtures.ts --self-test`; `bun run scripts/testing/baseline.ts --self-test`; `bun run --bun tauri build --no-bundle --config src-tauri/tauri.test.conf.json`; owner-observed isolated native launch and restore of a sacrificial fixture.
  - Fails if: the test identifier alone still reads real keyring entries, logs content/secrets, or release measurements require debug-only counters. These scripts/config/flags are proposed deliverables, not existing commands.
- P5 [ISOLATE]: Capture cache-empty/warm/OS-cache distinctions and fixture fingerprints; separate network timings; submit measured bottlenecks and proposed targets/budgets to the user.
  - Depends on: P4. Location: baseline runner/report and working plan target gate.
  - Verify: rerun same seeded fixture twice with matching results/command counts; instrumented native release traces from request through first selectable rendered list and full completion. Use five warmups/twenty short samples and ten expensive samples unless variance supports a documented amendment.
  - Fails if: timings are noisy/unattributable or absolute targets have no fixture/build/hardware/cache-state definition.

## Acceptance
- A1 (static + fixture): P1-P3 checks and discovery comparison → R1/R2/I2.
- A2 (fixture + native owner-observed): fixture/profile self-tests and inspected storage paths prove isolation/restoration → R3/I1.
- A3 (native instrumented release): replayable baseline and user-ratified target/fixture pairs, or explicit remaining gate → R3/I1/I2.

## Stop and rollback
Stop on unauthorized downloads, real-data paths, unexplained baseline failures, missing required host or unrepeatable measurements. Restore only owned test/instrumentation files; delete only marked disposable fixture/cache outputs after approval. Do not repair product bugs here. Report unavailable native checks explicitly.

## Revision log
- 2026-10-02: Initial proposed foundation; no tests/builds/measurements performed while planning.
- [REV 2026-10-02] Independent review: own the known Windows junction-test `rtk` dependency as test-only infrastructure; preserve native assertions and record the pre-fix baseline.

- [REV 2026-10-03] Execution on the user-confirmed Linux-only host: Windows native checks are unavailable and remain unverified. P1 found two test-only Windows assumptions: signalled POSIX processes have no exit code, and Linux Git preserves CRLF without the Windows carriage-return substitution. P2 includes platform-tagged assertions preserving the existing Windows expectations; no production runner behavior changes. Native render measurements and target ratification remain an explicit gate.

- [REV 2026-10-03] P5 native Linux release collection found a 300 ms workspace-status timer and focus refresh could race benchmark exit, adding one unrelated Git status command. The opt-in benchmark workload suppresses those background probes; normal builds retain them. Repeat seeded collection and compare independent counters before accepting extraction evidence.
