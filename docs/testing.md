# Isolated testing

Run frontend and native unit checks from the repository root:

```sh
bun run --bun check
bun test src/lib
bun run --bun build
bun run scripts/testing/css-order.ts --self-test
bun run scripts/testing/css-order.ts
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --features test-profile --lib -- --test-threads=1
cargo clippy --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git -c core.whitespace=cr-at-eol diff --check
```

The Bun loader compiles project Svelte rune modules for server tests. These tests do not establish client effects or native filesystem guarantees. Shared Windows layout vectors live in `src/lib/test-support/layout-vectors.json`;
platform-tagged Windows/Linux vectors live in `src/lib/test-support/platform-layout-vectors.json`.

The CSS manifest preserves fourteen source chapters in their original order. The checker
normalizes line endings and project-relative asset URLs, then verifies the reconstructed
source hash. Its self-test rejects reordered/changed chapters and changed asset references. An intentional
CSS change needs a reviewed manifest update; do not regenerate hashes to conceal drift.

Strict Clippy currently reports Linux dead code, including public compatibility aliases.
`-D warnings -A dead_code` is a diagnostic check only; it does not satisfy the strict gate.

## Disposable Linux release measurements

Use new absolute paths for `ABS_FIXTURES` and `ABS_PROFILE`. The scripts refuse existing or linked roots. Fixtures contain local Git repositories only. Preparation verifies a sacrificial backup restore.

```sh
bun run scripts/testing/fixtures.ts --self-test
bun run scripts/testing/native-profile.ts --self-test
bun run scripts/testing/baseline.ts --self-test
bun run scripts/testing/fixtures.ts --create ABS_FIXTURES 512
bun run scripts/testing/native-profile.ts --prepare ABS_PROFILE ABS_FIXTURES
bun run --bun tauri build --no-bundle --config src-tauri/tauri.test.conf.json --features test-profile -- --offline --locked
bun run scripts/testing/baseline.ts --collect ABS_PROFILE ABS_RELEASE_BINARY
bun run scripts/testing/baseline.ts --report ABS_PROFILE --cache app-cache-empty --os-cache uncontrolled
```

Run collection from a Linux desktop session with its display variables. The runner requires the compiled test-profile marker and testing identifier. It isolates config, data, cache and each sample’s WebView storage, preserves HOME, and uses generated fixture source IDs under a separate keyring service. Legacy credential migration is disabled only in test-profile builds. Normal builds exclude benchmark commands.

Benchmark mode suppresses focus-triggered and delayed workspace-status probes to keep comparison command counts reproducible. Normal builds retain those probes.

Collection starts five warmups and twenty measured fresh processes. `--expensive` uses five warmups and ten measured processes. Reports require completed result hashes, finite phase observations and matching independent Git command counters. The first-render measurement requires a connected, sized and selectable comparison row. Memory is sampled process-tree RSS; shared pages can be counted twice and brief peaks can be missed.

Fresh application storage is not OS-cold cache. The runner currently measures `app-cache-empty` only. Same-process warm-cache and network measurements remain unmeasured. Editor acceptance below
proves rendering and refusal, without ratifying editor latency targets. Logs contain fixed phase/operation names, timings, counters and hashes, without command arguments or file contents. Retained evidence belongs under ignored `.skillify/evidence/`; fixtures and profiles must never point at real repositories or settings.

Windows write, junction and recovery checks require native Windows. Linux compilation and server tests do not substitute for them. Current Linux save/copy/recovery support remains gated by the Linux protection contract and later backend packets.

See [packet execution status](implementation-status.md) and the [Linux write contract](linux-write-contract.md)
for completed work, deferred checks and the accepted practical contract.

## Linux read-only editor acceptance

Prepare a separate disposable profile with the commands above. Before launching, set its
`benchmark-plan.json` to select HEAD versus the working tree:

```json
{"left":{"setId":"fixture-set","itemId":"fixture-item","reference":{"kind":"head"}},"right":{"setId":"fixture-set","itemId":"fixture-item","reference":{"kind":"workingTree"}},"scenario":"linux-read-only"}
```

Launch the instrumented release artifact from the desktop session:

```sh
bun run scripts/testing/native-profile.ts --launch ABS_PROFILE ABS_RELEASE_BINARY 1
```

This test-only scenario waits for a connected, sized editor containing both texts.
It verifies read-only options and zero edit tickets, then invokes all ten file/copy/recovery
commands. Each native command must refuse with a Linux capability reason. The result JSON
records these observations under `proof`. It does not establish native write support,
Windows ticket behavior or editor performance improvement.

Packet09 browser fixtures additionally cover new/foreign root selection, disabled Linux
commands and shortcuts, trash controls, supported Windows ticket acquisition, failed-ticket
read-only rendering, and cleanup/acknowledgement of recovery records whose roots are gone.
Windows browser IPC is mocked; required native Windows acceptance remains unavailable.

## Private Linux credential acceptance

The reference Linux drill needs the installed KSecrets daemon, Python D-Bus/PyGObject,
AT-SPI and a native Wayland session. It creates a private wallet and buses inside a fresh
marked test profile; it never activates, locks or edits the real desktop wallet. Keep
HOME unchanged and use a short absolute profile path for the Unix socket.

```sh
bun run scripts/testing/native-profile.ts --prepare ABS_PROFILE ABS_FIXTURES
bun run --bun tauri build --no-bundle --debug --config src-tauri/tauri.test.conf.json --features test-profile -- --offline --locked
python3 scripts/testing/linux-credentials.py --profile ABS_PROFILE --binary ABS_DEBUG_BINARY
```

The profile must not already contain a wallet. The runner restores sacrificial settings
before creating it, generates secrets in memory and passes app secrets through stdin.
A native wallet dialog may appear. The private accessibility connection supplies only
its generated password. The runner owns ports 5951 and its recorded child processes.
`credential-acceptance.json` records persistence, restart, locking, unavailable service,
access denial, deletion/recreation and local HTTP 401/403 checks without secret values.
Only generated fixture credential IDs are deleted. Private empty wallet files remain
inside the ignored profile for inspection.

This proves the native Linux backend in that private environment. It does not configure
the production desktop's Secret Service, exercise real GitHub credentials or replace
Windows/owner-observed acceptance. [Credential contracts](linux-credentials.md) describe
uncertain mutation responses and the process-local revision seam.

## Cold-path contract controls

Packet17 controls live in `compare::tests::cold_path`. A clean CRLF checkout still
compares its raw bytes against the LF commit blob. Porcelain status and raw Git diff
both omit that file. A clean-filter sentinel proves those commands execute filters
which comparison avoids.

Independent SHA-1 and SHA-256 repositories preserve raw equality, normalized equality,
line counts and content reads. One Git object database cannot run tree diffs or batch
read both formats, even with object alternates. Endpoint-owned batch readers keep these object databases separate. Working-tree
inventory hashes raw bytes in each endpoint's object format without invoking filters.

```sh
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib cold_path -- --test-threads=1
```

The frozen packet 17 oracle lives in `compare/tests/legacy`. Equivalence tests compare
serialized rows, summaries, options and history while excluding generated file IDs.
Fixtures cover renames, deletions, type changes, binary files, BOM/CRLF, filters, links,
submodule entries, empty trees, unborn HEAD, content limits and independent object formats.
Lifecycle controls verify reader release on close, cancel, refresh and owner drop.

```sh
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib compare::tests -- --test-threads=1
bun test src/lib/content-bytes.test.js
```

The comparison content command sends raw binary through `tauri::ipc::Response`.
The API adapter returns a `Uint8Array` and derives content metadata from the request
and selected row. The backend still validates the session, generation, source and bytes.

Rust counts cover disjoint LF edits when a cached `git check-attr -a` probe finds no
applicable attributes for `left` and `right` in the private storage root. The probe
uses the same `core.attributesFile=` override as the legacy no-index diff. It also
reads effective `core.autocrlf` once from the storage working directory. Rust counts
preserve CR bytes under `false`; `true` and `input` convert CRLF when there is no lone
CR or NUL in the first 8000 bytes. Other CR-bearing inputs retain Git counts.
Eligibility and diff configuration reset on accepted open and refresh, before
materialization. Open never waits for a fetch. Storage ancestors with `.gitattributes` keep
the Git fallback because attributes can target generated child directories.
Batch readers have twelve independent permits;
same-Git-directory endpoints share one reader. Readers release after 30 seconds idle
and restart once after a read error. They close on session release, cancellation,
refresh, root mutation, fetch and pull. Page reload drains sessions synchronously
before scheduling reader closes. Linked worktrees use their Git and common directories.
Long-path inventories retain name-status metadata at the existing capture limit.

The ignored `compare::tests::cold_measure::measure_cold_comparison` test provides a
backend-only before/after harness with the `benchmark` feature. It requires
`SKEIN_COLD_ROOT`, `SKEIN_COLD_ENGINE` (`old` or `new`),
`SKEIN_COLD_WORKLOAD` (`refs`, `cross`, `working` or `working-crlf`) and `SKEIN_COLD_RESULT`.
The CRLF workload uses `checkouts/crlf`, with CRLF text and `core.autocrlf=true`.
Set `GIT_CONFIG_GLOBAL` to an isolated configuration with `core.autocrlf=true` so
the private storage also uses automatic conversion.
`SKEIN_COLD_NORMALIZE_EOL=false` disables EOL normalization; it defaults to `true`.
Use fixtures with `.skein-disposable` containing `skein-disposable-fixture-v1` and output paths inside the owned worktree. Capture process-tree
RSS only between its `MEASURE_BEGIN` and `MEASURE_END` markers. These measurements
exclude WebView rendering and do not replace native release or Windows/Defender evidence.

## Linux Git helper cleanup

Run the native Git tests serially. They fork only inside marked private fixtures and clean
recorded pidfd identities. The unprotected control intentionally demonstrates a held-pipe
leak before cleanup; protected tests cover completion, timeout, repeated cancel, dropped
callers, unrelated survival and the retained-child fallback.

```sh
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib git:: -- --test-threads=1
```

[Linux process ownership](linux-git-lifecycle.md) explains the kernel requirements, retained
leader ordering and detached-helper limit. Native comparison regression uses two fresh
instrumented release samples and checks result fingerprints plus command counts; it does
not establish performance improvement or Windows acceptance.

## Linux recovery library acceptance

[Linux recovery](linux-recovery.md) documents the private namespace, accepted interruption
boundaries, conditional undo, conflict handling and owned cleanup. Run its serial native
commands in an isolated worktree. The combined packet13 checkout passes118 default and
123 test-profile tests, with one ignored subprocess helper, plus63 frontend tests.
The frozen repair evidence includes41 owned SIGKILL/restart cases and four independent
regression rechecks. These prove process interruption and fixture recovery, without
proving power-loss durability or enabled application workflows. Application write
commands remain disabled until packet14 acceptance.

## Durable Linux comparison storage controls

Run the serial native controls from the owned isolated worktree:

```sh
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib linux_diff -- --test-threads=1
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib compare -- --test-threads=1
```

Fixtures live under `.skillify/evidence/paperwing/14/resume/native`, carry an ownership
marker, prove restoration before mutation and retain before/after source and sentinel
checks. Tests cover guarded missing suffixes and create races, direct/external metadata
and bind overlap, durable quota/restart/partial accounting, malformed artifacts, two
owned processes racing missing locks, bounded contention, cancellation, torn publication,
exact cleanup and retained failure charges. A user namespace supplies the foreign-owner
control. Four dropped leases retain their original work permits while another process
holds the lock. A separately delayed storage worker leaves a current-thread heartbeat
running. Shutdown without cleanup dispatch retains artifacts and releases capacity.

Quota controls reproduce namespace growth from4096 to12288 bytes and the former8192-byte
overcommit before the repair. The amended inventory prepays1MiB across restart and churn.
Native controls cover insufficient prepayment before initializer creation, non-ext4
refusal, late directory/manifest headroom exhaustion, growth postcondition failure, and
retention when native allocation overhead exceeds its reservation. Owned artifacts remain
while work permits return. The host verifies4096-byte ext4; unsupported block sizes are
rejected by the fixed qualification check and are not mounted in these native fixtures.

The former `compare::text_diff::temporary_tests` privacy check now runs against the final
lease as `linux_diff::tests::private_materialization_cleans_only_its_exact_files_and_releases_capacity`.
Comparison discovery also adds missing-configuration/source-authority refusal. Final
native two-process line-count controls assert added2/removed1 and exactly one diff
operation per process. Instrumented release replay uses fresh marked fixtures/profiles
and the existing native-profile runner, comparing every operation counter and the exact
result fingerprint. No performance improvement or enabled application-write claim follows
from these checks. Frozen packet14 resume evidence records actual commands and counts.

## Guarded missing-parent controls

Run the guard and journal suites serially in the owned packet14 worktree:

```sh
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib linux_guard -- --test-threads=1
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib linux_journal -- --test-threads=1
```

New fixtures use absolute marked14 paths, prove restoration before mutation and retain
source/sentinel snapshots. Existing packet12/13 helper names run only below the new checkout.
The parent subprocess helper is ignored during discovery and invoked only by its owned
SIGKILL matrix. That matrix covers76 intent, mkdir, persistence, publication and cleanup
boundaries. Restart assertions compare complete created subtrees and exact parent outcomes.
Additional controls inject changed roots, metadata, proven prefixes and absent, pending,
corrupt, foreign or mismatched linked file intents. Cleanup reports removed record files.

The ignored `linux_journal::parent_tests::frozen13_fixture_producer` runs only with explicit
`SKEIN_PARENT_COMPAT_FIXTURE` and typed `SKEIN_PARENT_COMPAT_CASE`. Its four cases are
`proofWithDirectory`, `proofOnly`, `laterEditWithDirectory` and `laterEditProofOnly`.
Each uses real guarded publication and a distinct real parent-cleanup proof. Producer and
consumer processes run sequentially at the same absolute fixture path after releasing flock.
The exact frozen13 consumer is
`linux_journal::compatibility_tests::packet14_foreign_parent_artifacts_preserve_verified_r_undo`.
The owned compatibility checkout preserves every original frozen production byte, adds only
a test module declaration and the harness, and uses its own14 target directory. Run only
that exact consumer; broad frozen suites would write historical evidence. The four rows
prove foreign-artifact preservation, unchanged r-format export/undo, later-edit refusal and
blocked ordinary admission. They do not enable application writes or replace independent review.

## Windows diagnostics

Download the `skein-windows-diag-nsis` artifact from the GitHub Actions run that built the test
version, then install that NSIS installer on the Windows test machine. Use Skein normally while
the diagnostics build samples process and benchmark data in memory.

In Settings, choose **Generate diagnostics**, review the anonymous JSON preview, and save the
file when ready. Skein does not upload it. Send the saved file privately through the approved
support channel; do not attach it to a public issue or commit it to the repository.
