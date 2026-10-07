# Skein handoff, 2026-10-07 06:36 (Europe/Bucharest), from the main session (paperwing-53, ~/.claude)

Read this first, then `plans/README.md`. The overnight running log is `plans/2026-10-06/handoff-main.md`. The alt session (claude-alt, paperwing-c9) cannot see this session in ListAgents: talk through `plans/2026-10-06/parallel-claims.md`.

## OWNER INSTRUCTION (2026-10-07, ~06:45): merge everything and produce a new build
The owner said: "tell him to merge the remaining work into main so a new build will emerge."
1. When each running fix finishes, verify the diff, rerun the gates outside the Codex sandbox and run the reviewer. Then merge into main:
   - packet 42: `crew/api-builder-k5ts0`, base 5fad9ce
   - packet 33: MERGED to main (07:00)
   - packet 17: `crew/api-builder-j6wme`
   Merge 42 and 33 first and 17 last. 17 and 42 both touch `git/runner.rs`.
2. Run the full gates on main: `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`, and `cd src-tauri && cargo test --offline` (SKEIN_TEST_TMP absolute, no `..`). Also run `cargo test --offline --features diagnostics`.
3. Push main to origin. The owner asked for a new build, and this is that authorization. CI builds the `skein-windows-nsis` and `skein-windows-diag-nsis` artifacts. Check the run, then give the owner the run link and both artifact names.
4. If a fix fails review twice, or blocks for long, do not hold the build. Merge what is green, push, and leave the rest on its branch with a note.

## Owner decisions this session
- Anonymous diagnostics export for Windows test builds (packet 41).
- Proactive UI, backend and architecture improvements are allowed. New visual directions go behind a picker Artifact first.
- Codex implementation runs use gpt-6.1-sol xhigh only. Luna is for scout and research (`~/.config/crew/roles.json` updated).
- Shell redesign round 1 rejected as too compact. Keep today's repo look. Round 2 is assigned to the alt session on Opus (`plans/2026-10-07/shell-redesign-brief.md`).
- Comfortable density is the default for every set (373126b).

## Done: merged to local main (38 commits ahead of origin/main, not pushed)
- Perf work: perf-frontend, perf-backend, review fixes (e71de6c; this one is pushed).
- Packet 39: branch cleanup dialog and code search (ae99ff6).
- UX polish batch A (17a2a67): load and error states, tooltips, type floor, density, set compare, ref picker refetch loop.
- UX polish batch B (bca0944): one primary per screen, bulk bar, palette, table keyboard, error copy, progress.
- Packet 41 diagnostics: backend (be5223d) and Settings UI (28010fe). Only the Windows proof is missing.
- Packet 26: stash UI and switch with stash (bcfe546).
- Packet 28: pull request column, open dialog, bulk open (57cd8d9). The rail badge waits for the shell pick.
- Packets 40, 41, 42 written; audits saved in `plans/2026-10-06/audits/` (architecture, ux).

## In progress
| What | Where | State |
|---|---|---|
| Packet 17 round 2 (faster compare) | Codex `crew/api-builder-j6wme`, `.crew/paperwing-api-builder-j6wme` | Running. Fix list in packet 17, "Phase 1 review round 2". Then review, then merge. Base 9eeecd5. |
| Packet 42 backend hardening | `crew/api-builder-j6sse` at 5fad9ce (main merged in, gates green: cargo 514, bun 326) | Review: changes-needed (legacy token hides Git output, settings error aborts startup, stash retry too broad, busy slot on save, store Condvar on async threads, weak tests). Fixes running on Codex `crew/api-builder-k5ts0`, base 5fad9ce. Then merge. |
| Packet 33 tags | UI `ui/33-tags` 7f0f83e; backend plus main merge `crew/api-builder-j8evh` 0c38db7 (cargo 507, bun 339) | Review: approve-with-nits. Nits running on Codex `crew/api-builder-k0exg` (base 0c38db7). Merge after that; it contains the UI. |
| Shell redesign round 2 | alt session, `.alt/shell-design-2/` | Alt posts the Artifact link in `parallel-claims.md`. |

## Waiting on the owner
1. Pick a shell direction from round 2, when the alt session publishes it.
2. Push allowed by the owner for the new build (see the owner instruction above).
3. Packet 40 order: right after the speed work?
4. Packet 27: snapshots in SQLite, fine?

## Next steps, in order
1. On each report, verify the diff and gates, run the reviewer, then merge (17, 42, 33).
2. After 17 merges: write packet 43 (audit items 2, 8, 9, 10, 12, 13, 19; Git locale pin in the runner), then run it on Codex.
3. After the push is allowed: watch CI, install on the Windows VM (VNC 127.0.0.1:5927), smoke-test, give the owner the diag installer link.
4. Ready packets without the shell pick: 38 (core boundaries, Codex), 29 (discard and partial staging, backend first), 30 (auto-refresh, backend), 06 and 24 (need the VM).
5. After 17: 37 + 06s (editor and full-screen compare), 40 (compare on GitHub).
6. After the shell pick: write the shell packet; then UX-01, 12, 13, 17, 18, 24, 25; packet 03b; the packet 28 rail badge; the search follow-ups in the backlog.
7. Later follow-ups:
   - Backend `target_repo` lookup for forks (packet 28).
   - `user-select: none` on app chrome; Ctrl+A with no row focused selects page text.
   - Palette ranking polish.

## Cleanup when safe (ask before removing anything you did not create)
- Empty or finished crew worktrees: 16dhj, 385q5, 1ci2m, 24d6y, 3whm7, j73bl (empty), 3b438 (superseded by j6sse), 368uj (superseded by j6wme after merge).
- Merged UI worktrees: ui-39, ui-41, ui-polish-a, ui-polish-b, ui-26, ui-28, merge-perf.
- Alt-owned (`.alt/perf-*`, `check-slow`, `rename`, `decisions`, `win-store`, `shell-2-base`): the alt session decides.

## Notes
- `SKEIN_TEST_TMP` must be an absolute path with no `..`, or tests fail with "Traversal is not supported".
- Codex sandbox runs show 3 environment-only Rust failures (newuidmap, ACL) and esbuild errors. Rerun the gates outside the sandbox before believing a failure.
- Crons in this session are session-only. The 30-minute alt check dies with this session.
