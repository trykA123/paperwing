# Packet 38 implementation report

Implemented in `crew/api-builder-p38-core`, using the orchestrator's app-crate
fallback. Changes remain in the working tree. No git metadata writes, commits,
pushes, merges, deployments or other-checkout edits occurred. The previous stop
report remains as historical evidence.

## Steps

| Step | Change | Check result |
| --- | --- | --- |
| 1 | Added `src-tauri/src/core/` for events, capabilities and lifecycle. Existing domain modules remain in the application crate. Documented the later `skein-core` extraction. | Core has no Tauri imports. Linux workspace tests retain all 609 existing tests and add eight. Native Linux debug Tauri build passes with Node frontend preprocessing. Standard Bun build gates encounter the sandbox's esbuild failure. |
| 2 | Nine domain modules export handlers; `lib.rs` composes their command registration. | 82 unique names, 102 registrations and 102 function signatures match the baseline. Both inventory diffs are empty. Optional feature handlers compile. |
| 3 | Producers publish serializable `CoreEvent` values through Tokio broadcast. One app forwarder preserves existing frontend event names and payloads. | Bus round-trip and mock Tauri listener tests both pass for all 11 legacy names. All 398 frontend tests pass; frontend test files are unchanged. |
| 4 | Added `RepositoryProvider`, `PullRequestProvider`, `CiProvider`, `IssueProvider`, cancellable registry leases and GitHub adapters. Instances use configured source ID and host. | Registry tests prove separate cloud and Enterprise instances, reuse, disabled construction prevention and cancellation. Counting transport tests prove zero disabled requests and zero rows across all six cache tables, including rejected late listing writes. Existing GitHub tests pass. |
| 5 | Migration 2 persists enabled flags. Disabling cancels requests and purges provider caches. Settings adds one existing-style toggle per source; API types add optional `enabled`. | Backend settings commands prove off → store restart → still off, SQLite precedence and reenable. Counting transport proves listing resumes after enable. Node Svelte check passes. Browser keyboard and screenshot verification is blocked before browser startup. |
| 6 | Added `docs/architecture.md` and one changelog entry. | Documents layers, typed traits, lifecycle, event compatibility, persistence, later crate extraction and all ten questions from `TEMPLATE.md`. |

## Inventory and platform preservation

Evidence lives in [packet38-core-boundaries-evidence](packet38-core-boundaries-evidence/).
`commands.diff` and `signatures.diff` are both zero bytes. The inventory is
74 default names, 82 including optional features, and 102 platform-inclusive
registrations. `src/lib/api.ts` changes only the new optional provider setting.

Run `python3 plans/2026-10-07/packet38-core-boundaries-evidence/verify-inventory.py`
from this worktree to reproduce the inventory and preservation assertions.

Only these ten Windows registration entries moved from `lib.rs` to
`commands/files.rs`; their complete lines, including cfg attributes and CRLF
endings, are byte-identical:

```text
files::file_edit_open
files::file_edit_close
files::file_save
files::copy_preview
files::copy_apply
files::copy_cancel
files::recovery_list
files::recovery_undo
files::recovery_cleanup
files::recovery_resolve
```

No Windows functions or platform implementation blocks moved. The preservation
script verifies 23 protected files byte-for-byte, including native file modules,
file guards, commit and stash. The compare editor files are unchanged. Production
host selection uses configuration; the cloud host appears only in preexisting
logic and explicit test fixtures.

The forwarder retains `discover-batch`, `discover-done`, `search-matches`,
`search-repo`, `search-done`, `clone-progress`, `clone-finished`, `launch-request`,
`credential-changed`, `git-activity` and optional `diagnostics-progress`.

## Checks

Rust tests used these absolute worktree paths on verified ext4:

```text
SKEIN_TEST_TMP=/mnt/Sabrent/homelab/.crew/paperwing-api-builder-p38-core/.packet38/test-tmp
TMPDIR=/mnt/Sabrent/homelab/.crew/paperwing-api-builder-p38-core/.packet38/test-tmp
```

The generated scratch directory was removed after checks finished.

| Command | Result | Evidence |
| --- | --- | --- |
| `cd src-tauri && cargo test --offline --workspace` with the environment above | Exit 101: 607 passed, three existing environment failures, seven ignored; 617 total. Baseline: 599 passed, the same three failures, seven ignored; 609 total. | `rust-tests-final.log`, `before-tests.log` |
| `cargo test --offline --workspace --lib -- core::registry::tests events::tests github::provider::tests github::http::tests::disabled settings::provider_tests` with the same environment | Exit 0: all eight new tests pass after the final lint edits. | `new-tests-final.log` |
| `cargo clippy --offline --workspace --all-targets --message-format=json` | Exit 0. All 30 warning diagnostics match the baseline; zero new warnings. | `clippy-after.log`, warning JSON files, empty `clippy-warnings.diff` |
| `rustfmt --edition 2021 --config skip_children=true --check <all 23 new Rust files>` | Exit 0. | `rustfmt-new-files.log` |
| `cargo check --offline --workspace --all-features --all-targets` | Exit 0 on Linux. | `all-features-all-targets-check.log` |
| `bun test src/lib` | Exit 0: 398 passed, zero failed, 2,342 assertions across 41 files. | `bun-tests-final.log` |
| `bun run --bun check` | Exit 1: esbuild service stops; three preprocessing errors in BrandMark, WindowControls and Tabs. Reproduced with isolated dependency files. | `frontend-check.log`, `frontend-check-isolated-bun.log` |
| `node ./node_modules/svelte-check/bin/svelte-check --tsconfig ./tsconfig.json` with isolated dependency files | Exit 0: 294 files, zero errors and warnings. | `frontend-check-node-final.log` |
| `bun run --bun build` | Exit 1: esbuild service stops. | `frontend-build.log` |
| `node ./node_modules/vite/bin/vite.js build` with isolated dependency files | Exit 0; existing large Monaco chunk warning. | `frontend-build-node.log` |
| `bun scripts/testing/css-order.ts` | Exit 0; stylesheet order unchanged. | Terminal result |
| `bun run tauri build --no-bundle` | Exit 1 before native compilation: Bun frontend build hits esbuild failure. | `tauri-build.log` |
| `bun run tauri build --no-bundle --debug --config .packet38/node-build.json` | Exit 0: Node frontend build and Linux native executable build. Override changes only the preprocessing command. | `tauri-build-node-debug.log`, preserved `node-build.json` |
| `git -c core.whitespace=cr-at-eol diff --check` | Exit 0. Existing CRLF files and moved registration bytes are preserved. | Terminal result |
| `python3 .../verify-inventory.py` | Exit 0: empty command/signature diffs; ten unchanged Windows registration lines; 23 protected files unchanged. | `inventory-summary.json` |

The unchanged environment failures are:

- `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
- `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: ACL operation returns `EINVAL`.
- `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: ACL operation returns `EINVAL`.

## Measurements

Paired warm-dependency debug builds ran
`cargo build --offline --workspace --features tauri/custom-protocol` with
`CARGO_INCREMENTAL=0`, identical frontend assets and shared compiled dependencies.
The baseline snapshot came from read-only `git show HEAD` operations inside this
worktree. Both builds exited 0.

| Metric | Before | After | Change |
| --- | ---: | ---: | ---: |
| Native compile time | 14.63 s | 13.82 s | -5.54% |
| Executable size | 342,143,176 bytes | 345,882,624 bytes | +1.09% |

Both measured changes stay below the packet's 20% stop threshold. Release timing
and size were not measured. See `native-build-measurements.json` and native build
logs.

## Files and remaining verification

The complete 52-file product/docs/test list is in
[changed-files.txt](packet38-core-boundaries-evidence/changed-files.txt).
New files are under `core/`, `commands/`, event/provider adapters and their tests,
plus the provider migration and architecture document. Existing changes are
limited to setup, event producers, GitHub and ref delegation, settings, store
migration registration, fixture updates, the changelog and the two allowed
frontend files. Cargo manifests, lockfile and app identity are unchanged.

Windows CI remains required. Real Settings screenshots and keyboard checks remain
required: preview binding on `127.0.0.1:41000` failed with `EPERM`, and Helium
startup failed with `setsockopt: Operation not permitted`, including with a
writable isolated profile. No page rendered and no screenshots were produced.
The attempted browser harness and failure logs are preserved as evidence.

The standard Bun check/build and release Tauri build gates remain blocked by the
sandbox's esbuild failure. Node check/build and the native debug build passed.
The three baseline environment test failures require an unrestricted Linux rerun.
No gate above is reported as passed without execution.

The original `node_modules` symlink is restored. All generated scratch files are
removed. No server remains on ports 41000–41019.
