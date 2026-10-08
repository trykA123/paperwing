# 43 — Git runner hardening and Windows spawn cost

Status: ready (packet 17 phase 1 is on main)
Platform: Windows first, Linux parity
Size: M
Role: api-builder (gpt-6.1-sol xhigh), one writer

## Goal
Every Git call on Windows costs one process start instead of two, Git output is stable in any locale, and the runner stops flooding the UI and the CPU. On the owner's work PC each spawn costs 150-320 ms today (`plans/2026-10-07/diagnostics-owner-1.md`), so this is the cheapest speed win left.

## Already done
- Runner: `src-tauri/src/git/runner.rs` (32 slots, batch readers from packet 17 in `git/batch.rs`).
- The audit with file and line references: `plans/2026-10-06/audits/architecture.md`, items 2, 8, 9, 10, 12, 13 and 19.

## Decisions
- Do the audit items exactly as their "Fix" column says, in this order: 8, 10, 2, 12, 9, 13, 19.
- Item 8: resolve the real `git.exe` once (`git --exec-path`, fall back to `git`), cache it in a `OnceLock`, and prepend `<prefix>\mingw64\bin;<prefix>\usr\bin` to `PATH` for the child, as the launcher does. SSH, Git Credential Manager and HTTPS proxy settings must keep working.
- Pin the Git locale for every child: `LC_ALL=C`, `LANGUAGE=` (empty). Decisions already use exit codes; this keeps the text stable for logs.
- No behaviour change beyond the audit items. The command inventory (82 names, 102 registrations) stays identical.

## Scope
- Do: the seven audit items and the locale pin, each with a test.
- Do not: priority classes (packet 19 owns them), new commands, UI beyond the activity delta in item 9.

## Read first
- `plans/2026-10-06/audits/architecture.md` (items above)
- `src-tauri/src/git/runner.rs`, `git/redaction.rs`, `git/repository_tree.rs`, `git/remote_refs.rs`, `local.rs`, `test_support.rs`
- `src/lib/state/git-activity.svelte.ts`
- `~/.agents/rules/rust.md`, `~/.agents/rules/code-quality.md`

## Steps
1. Item 8. Check: a test with a fake exec path proves one child per call and the `PATH` prefix; Windows CI `test-windows` passes.
2. Item 10 (`RepoGit` with `core.fsmonitor=false`). Check: the hook fixture's marker file stays untouched after `local_status`.
3. Item 2 (non-blocking `safe()`). Check: hold a credential permit; `safe()` still returns the redacted original text.
4. Item 12 (`git remote -v`, tag peel map). Check: parity test for multi-URL remotes and `insteadOf`.
5. Item 9 (activity deltas, at most 10 events/s per job). Check: a 500-line status produces at most 3 events; the Activity panel still shows every line.
6. Item 13 (cancellation without 25 ms polling). Check: 800 queued jobs cause no timer wakeups while idle.
7. Item 19 (runner lock assertion, one test helper). Check: a test that spawns Git without the lock fails.
8. Locale pin. Check: a test asserts the child environment.

## Done when
- A Windows diagnostics export shows Git spawns cost about half of today's 150-320 ms.
- All gates pass on Linux and in Windows CI.

## Gates
- `cd src-tauri && cargo test --offline` (`SKEIN_TEST_TMP` on ext4, never `/tmp`), `rustfmt --check` and clippy on touched files
- `bun test src/lib`, `bun run --bun check`

## Stop and report if
- The real `git.exe` cannot be resolved reliably (portable Git, Scoop, a custom install), or GCM/SSH stop working without the launcher.
