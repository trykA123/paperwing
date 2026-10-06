# 42 — Backend hardening from the architecture audit

Status: ready
Platform: Windows first, Linux parity
Size: M
Role: api-builder (gpt-6.1-sol xhigh)

## Goal
Fix the defects that the 2026-10-06 architecture audit found outside the packet 17 area: a token that can reach the wrong host, settings that a power cut can corrupt, UI freezes from commands on the main thread, and behaviour that breaks with a non-English Git.

## Already done
- Audit with file:line evidence: `plans/2026-10-06/audits/architecture.md`. This packet covers items 1, 3, 4, 5, 6 (outside the runner), 11 and 14.
- Items 2, 8, 9, 10, 12, 13 and 19 wait for packet 17 (they touch `git/runner.rs` or `compare/`). Item 7 belongs to packet 38 step 4. Items 16 and 17 are module splits for later. Item 18 is frontend autosave.

## Decisions
- Item 1: a saved token is bound to the host it was saved for. Store the host with the credential metadata. Every request using a saved token checks that the request host equals the bound host (case-insensitive). On a mismatch it refuses with "This token was saved for <host>. Save a token for <new host> first." Test and Load orgs on an edited, unsaved host use only a token typed in that form, never the stored one. Existing credentials with no recorded host take the source's saved host on first read. That is a migration, not a reset.
- Item 3: settings writes are durable. Write the temp file, `sync_all`, rename, then sync the directory on Linux. Keep `settings.json.bak` as the last good copy, written before the replace. Load falls back to `.bak` when the main file fails to parse, and the UI shows a toast "Settings were restored from a backup". If both fail, start with defaults and keep the broken file as `settings.json.broken-<timestamp>`. Never delete it.
- Item 4: every command that touches disk or does O(n) work is `async` with `spawn_blocking` for filesystem work: `start_clone`, `save_settings`, `load_settings` and any other sync command the audit lists. `start_clone` keeps calling the existing validation in `compare/registration.rs` unchanged, wrapped in `spawn_blocking`. Do not edit that file (packet 17). Remove `paths_exist` and its `api.ts` entry if nothing calls it. Run `valid_path` on the `open_in_vscode` path and make it async.
- Item 5: `load_settings` becomes a pure read. Applying sources (`credentials::configure_sources`, `git::configure_sources`) happens only at startup and in `save_settings`.
- Item 6, outside the runner: replace stderr text matching in `commit.rs`, `stash/ops.rs` and `state.svelte.ts` with exit codes, plumbing checks or structured error kinds. Pinning the locale in the runner env is packet 17 follow-up work; do not edit `git/runner.rs`.
- Item 11: the store start race. Calls before the store opens wait (bounded, 5 s) instead of returning `Unavailable`. Remove the commit cache writes if nothing reads them, or wire the read the audit names. Pick whichever the code supports with less change, and say which.
- Item 14: one shared object-id validator (40 or 64 hex) used by stash, tags, branch_cleanup and pulls.

## Scope
- Do: the items above, each with a test that fails before the fix.
- Do not: edit `git/runner.rs`, `git.rs`, `compare.rs` or `compare/` (packet 17), `src-tauri/src/diagnostics/` (packet 41), the search and branch-cleanup UI (packet 39). Do not change the settings JSON schema beyond adding the token host.

## Read first
1. `plans/2026-10-06/audits/architecture.md`
2. `src-tauri/src/github.rs`, `github/http.rs`, `credentials.rs`, `credentials_metadata_tests.rs`, `src/components/Settings.svelte` (Test and Load orgs)
3. `src-tauri/src/settings.rs`, `clone.rs`, `compare/registration.rs` (read only), `store/mod.rs`, `github/commit_cache.rs`
4. `src-tauri/src/commit.rs`, `stash/ops.rs`, `src/lib/state.svelte.ts` (around line 617)
5. `~/.agents/rules/code-quality.md`, `rust.md`, `typescript.md`

## Steps
1. Item 1 with tests: a saved token is never sent to a different host, migration of host-less credentials, and Test with a typed token on a new host. Check: `cargo test credentials github`.
2. Item 3 with tests: torn main file falls back to `.bak`; both broken leads to defaults plus a kept broken file. Check: `cargo test settings`.
3. Items 4 and 5. Check: a test or assertion that the listed commands are async, and the frontend still loads settings (`bun test src/lib`).
4. Item 6. Check: tests run Git with `LANGUAGE=de` and `LC_ALL=de_DE.UTF-8` where available, or simulate the localised stderr, and get the same behaviour.
5. Items 11 and 14. Check: a store test where a call made before open waits and succeeds; validator unit tests.

## Done when
All steps' tests pass; no behaviour changes beyond the decisions; and the Windows CI test job is green.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `cd src-tauri && cargo test --offline` (`SKEIN_TEST_TMP`: an absolute ext4 path with no `..`, never `/tmp`)
- `rustfmt --check` on files you create; clippy with no new warnings versus main
- Keep each file's line endings (`git show HEAD:<path> | file -`)

## Stop and report if
- Binding tokens to hosts needs a keyring layout change that loses existing tokens.
- A sync command cannot become async without a frontend contract change.

## Report
Commit sha (do not commit; leave changes uncommitted), files changed, each step's check result, gate results, anything skipped.
