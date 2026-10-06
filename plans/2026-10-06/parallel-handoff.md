# Skein: parallel work for a second Claude session (2026-10-06)

You are a second orchestrator working beside the main session (Claude Code, Opus 5.5, same machine, different account). The main session owns `feat/paperwing-linux-completion` and every merge into it. You build on your own branches and hand them back.

Read first:
- `~/.agents/AGENTS.md` and `~/.agents/rules/*.md` (binding).
- `plans/2026-10-06/orchestration.md` (tracks and decisions).
- `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md` (Skein design: Benzol, Geist, Rails icon, Formation).
- The packet you take, under `.skillify/plans/` (gitignored; read it in the main checkout).
- Memory: `~/.claude/projects/-mnt-Sabrent-homelab-paperwing/memory/MEMORY.md` (shared with the main session).

## Rules for this session
1. Work only in worktrees you create under `/mnt/Sabrent/homelab/.alt/` from the current `feat/paperwing-linux-completion` HEAD: `git worktree add -b alt/<packet> /mnt/Sabrent/homelab/.alt/<packet> feat/paperwing-linux-completion`. Run `bun install --frozen-lockfile` inside it (own `node_modules`).
2. Never edit, check out, reset or stash in a worktree you did not create. Never commit to or merge into `feat/paperwing-linux-completion`.
3. Commit on your `alt/*` branch with explicit `git add`, then add a line to `plans/2026-10-06/parallel-claims.md` in the main checkout (untracked file, append only): `<time> <branch> <sha> done|in progress — one-line summary`. The main session reviews and merges.
4. Claim before starting: append `<time> <packet> claimed by alt` to the same file. Skip anything already claimed there.
5. Do not use `crew`/Codex from this session (it shares the main session's Codex quota). Use Claude agents: `ui-builder` (Sonnet, medium), `ui-builder-high`, `designer`, `reviewer` (Opus), `scout`, `checker`.
6. Rust fixtures that need a writable root must not use `/tmp` (tmpfs; Linux writes require ext4): use `src-tauri/src/test_support.rs` once it is merged, else `PAPERWING_TEST_TMP=/mnt/Sabrent/homelab/.alt/tmp`. Use a separate `CARGO_TARGET_DIR=/mnt/Sabrent/homelab/.alt/target-<packet>`.
7. Gates before handing back: `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts` (+ `--self-test`), `cargo test --offline` (src-tauri), clippy clean in touched files, `rustfmt --check` on touched files, Helium screenshots for UI (390 and 1440, both themes), and a `reviewer` pass.
8. Publish finished UI prototypes as an Artifact and give the link (owner preference).

## Do not touch (in flight in the main session)
| Area | Files |
|---|---|
| Packet 14 fixes (Linux writes) | `src-tauri/src/linux_files/`, `compare/tests/linux_files.rs`, `test_support.rs`, `linux_guard/*`, `linux_journal/*`, `credentials.rs` strings |
| Packet 15 (clone, trash) | `clone.rs`, `trash.rs`, SetView removal dialog, `deleteSet` |
| Packet 34 (SQLite store) | `src-tauri/src/store/`, `github/cache.rs`, `github/listing.rs` |
| Skein step 3 (table, rail) | `SetView.svelte`, `components/set/*`, `Sidebar.svelte`, `RightPanel.svelte`, rail components, `src/lib/state/*`, `state.svelte.ts` |
| Editor spike, Jira prototype | scratch only (results come to the main session) |
| Windows VM | `.skillify/evidence/paperwing/windows/vm-20261006/` |

## Work you can take in parallel (pick top to bottom)
1. **Packaging and installers (packet 24 prep + packet 25 R6).** `tauri.conf.json` bundle settings, Windows NSIS per-user install with installer hooks that add and remove `HKCU\Software\Classes\Directory\shell\Skein` and `Directory\Background\shell\Skein` ("Open with Skein"), Linux `.desktop` with `MimeType=inode/directory` and a Dolphin service menu, AppImage/deb targets, and a GitHub Actions workflow that builds Windows and Linux artifacts on push to a branch (no release publishing, no version bump). Strip a trailing `"` from drive-root arguments is already handled in `launch.rs`.
2. **Packet 33 tags, backend.** New `src-tauri/src/tags.rs`: create (lightweight, annotated, signed when Git config already signs), push one tag, delete local, delete remote (separate command), name validation via `git check-ref-format`, refuse existing names; fixed argv; tests against a local bare remote. Register in `lib.rs` with minimal lines. UI comes after step 3 merges.
3. **Packet 26 stash, backend.** New `src-tauri/src/stash.rs`: list, push (message, include untracked), apply, pop, drop (explicit), show diff; "switch with stash" orchestration per repo with conflict reporting; tests.
4. **Packet 28 pull requests, client only.** New `src-tauri/src/github/pulls.rs`: PR for a branch, review state, combined checks; open PR (draft default); recorded-response test seam (no live network in tests); no caching (packet 34 will store results). Use the `skein-fixture-*` repos on the owner's GitHub only for a final manual check, and clean up any PR you open.
5. **Packet 31 Actions, client only.** New `src-tauri/src/github/actions.rs`: runs, jobs, logs (redirect), artifacts, re-run, re-run failed, cancel, dispatch with inputs parsed from the workflow file; ETag conditional requests; rate-limit backoff; recorded-response tests.
6. **Step 3b context menu component.** A standalone accessible `ContextMenu.svelte` (Shift+F10, Menu key, arrows, typeahead, Escape, focus return) with tests and a demo; do not wire it into the table until step 3 merges.
7. **Docs.** Rewrite `README.md` for Skein (what it is, install, Linux filesystem tiers from packet 32, Windows notes), plus `docs/user-guide.md`.

## Reaching the main session
Both sessions run on this machine. `ListAgents` shows the main session; `SendMessage` to it for questions or handbacks. If messages are held, the claims file is the fallback.
