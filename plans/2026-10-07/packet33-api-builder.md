# Packet 33 api-builder report

Branch: `crew/api-builder-j8evh`, base `7f0f83e`. Changes remain uncommitted.
The final run rule prohibits merging, so `git merge main` was skipped. No repository commit, push, or deployment ran.

## Changes and coverage

- `src-tauri/src/tags.rs`, `src-tauri/src/tags/tests.rs`: remote deletion requires a validated 40/64-hex expected object and uses an explicit force-with-lease. Bare-remote tests cover matching deletion, a moved tag retained, absent expected objects, and invalid identifiers. The two regression tests failed with the old unleased behavior temporarily restored and passed after restoring the fix.
- `src-tauri/src/github/releases.rs`, `src-tauri/src/github/releases/{client,repository,tests}.rs`, and three fixtures under `src-tauri/src/github/releases/fixtures/`: release command, default drafts, notes validation, configured host/source resolution, stored-token HTTP connection, annotated local tag and matching remote object checks. Nine release tests cover GHES source/API routing, rate limits with resetAt, permission denial, absent or moved remote tags, lightweight/missing/invalid local tags, push URL selection, explicit publication, and response URL validation.
- `src-tauri/src/github.rs`, `src-tauri/src/github/pulls.rs`, `src-tauri/src/github/pulls/{client,repository,source}.rs`, `src-tauri/src/lib.rs`: module/command registration and scoped visibility for existing source, transport, and remote resolution. Existing Pulls behavior is unchanged.
- `src/lib/api.ts`, `src/lib/tags-set.ts`, `src/lib/tags-set.test.js`, `src/components/tags/{TagDialog,TagReleases}.svelte`: IPC wrapper/type, eligible release runner, result-step controls, notes prefilled from the tag message, draft default, per-repository results and external links. Five new frontend unit tests cover eligibility, continuing after errors, rejecting ineligible rows, structured errors and explicit publication, and IPC defaults.
- `scripts/testing/browser/{tag-backend,ui-33}.mjs`: mocked release creation, real delete leases, and browser assertions for notes/defaults, push eligibility, GHES links, permission failure, and lightweight exclusion. Both scripts pass syntax checks; browser assertions could not execute in this sandbox.
- `README.md`, `plans/packets/33-tags.md`: release behavior and implementation notes.

Line endings are preserved. `diagnostics/`, `compare/`, and `git/runner.rs` were not edited.

## Verification

Every Rust test command used `SKEIN_TEST_TMP=/mnt/Sabrent/homelab/.crew/paperwing-api-builder-j8evh/src-tauri/target/test-tmp`; `findmnt` confirmed ext4.
Cargo commands ran from the worktree with `--manifest-path src-tauri/Cargo.toml`.

| Command | Result |
| --- | --- |
| `bun run --bun check` | Failed: sandbox EPERM when Bun writes subprocess stdin; esbuild stops. |
| `bun test src/lib` | Passed: 308 tests, 0 failures, 2088 assertions, 34 files. |
| `bun run --bun build` | Failed: same sandbox esbuild subprocess restriction. |
| `bun scripts/testing/css-order.ts` | Passed: manifest and flattened cascade match. |
| `cargo test --offline` | 502 passed, 3 failed, 7 ignored; all three failures reproduce on unchanged base. This full run preceded the two additional release URL/publication tests; focused runs below cover the final implementation. |
| `cargo test --offline tags::` | Passed: 12 tests. |
| `cargo test --offline github::` | Passed: 90 tests, including 9 release tests. |
| `cargo test --offline github::releases::` | Passed: final 9 release tests after extracting URL validation. |
| `rustfmt --edition 2021 --config skip_children=true --check <touched Rust files>` | 11 files pass; `src-tauri/src/lib.rs` fails unchanged formatting also present on base. Its command registration is the only edit. |
| `cargo clippy --offline --all-targets` | Passed: 23 distinct existing warnings, 0 new warnings against unchanged base. |
| `node node_modules/svelte-check/bin/svelte-check --tsconfig ./tsconfig.json` | Passed: 0 errors, 0 warnings. |
| `node node_modules/vite/bin/vite.js build` | Passed. Existing large Monaco chunk warning remains. |
| `node --check scripts/testing/browser/tag-backend.mjs` and `node --check scripts/testing/browser/ui-33.mjs` | Passed. |
| `git -c core.whitespace=blank-at-eol,blank-at-eof,space-before-tab,cr-at-eol diff --check` | Passed, preserving CRLF files. |

Requested gates: 3 of 7 pass in their requested form. Two Bun gates are blocked by sandbox subprocess restrictions. Full Rust tests and the format gate have confirmed baseline failures. Equivalent Node type/build checks pass.

The full Rust failures are:

- `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
- `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: kernel ACL operation returns EINVAL.
- `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: kernel ACL operation returns EINVAL.

Each failure was rerun against an isolated archive of the unchanged base inside this worktree and failed for the same reason. Baseline clippy and lib.rs formatting were also checked against that archive.

## Remaining acceptance

Vite on `127.0.0.1:41040` failed to bind with EPERM. The Helium browser fixture also failed with `spawnSync git EPERM`. No server started, and no screenshots or browser passes are claimed. The browser matrix and live GitHub/GHES release acceptance remain unverified.

Local Vite caches were isolated while testing; the original shared node_modules symlink was restored. No other checkout was edited.

Mapify deposit candidate: `create_github_release` in `src-tauri/src/github/releases.rs` resolves the push remote through `github/pulls/source.rs`, verifies its tag object in `github/releases/repository.rs`, and connects through the existing stored-token HTTP client. A read-only proposal was generated; no graph records were persisted.
