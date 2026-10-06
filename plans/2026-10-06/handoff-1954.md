# Skein handoff — 2026-10-06 19:54 (Europe/Bucharest), from the claude-alt session

The alt session took over orchestration while the main session was paused. Usage is at 90% of the five-hour window. Read `plans/README.md` (the packet index) and this file first.

## Owner priorities for tomorrow (said today)
1. Performance on Windows first. At work: no errors, but status checks were slow, and remote branches did not show in branch compare. That was an 800-repo GitHub Enterprise org and a ~1 GB repo.
2. Compare editor: "take the faster one". CodeMirror 6, plus the custom renderer for files over 5 MB. Recorded in packet 37.
3. Full screen means OS full screen. Recorded in packet 06s.
4. Wants packet 40: compare on GitHub without cloning (compare API, including GHES). Not written yet.

## Done (on `main`, pushed to origin unless noted)
- `main` is the only branch on GitHub. Local `main` is `d1ee281`, pushed.
- Merged today:
  - every alt branch: tags, stash, branch cleanup, search, PR client with GHES, packaging, CI, Windows title bar and grip, CI green
  - packet 34 store with review fixes
  - plans rewrite (`plans/packets/`, old `.skillify` packets deleted; backup `.alt/skillify-plans-backup-20261006.tgz`)
  - rename to Skein (`4fd9642`): persisted ids kept, listed in `docs/naming.md`
  - Windows store-handle fix (`d1ee281`)
- Branch backup list: `.alt/branches-backup-20261006.txt`. Restore with `git branch <name> <sha>`.
- CI run 37498431610 on `d1ee281`: test (Linux), linux and windows builds succeeded. test-windows also passed: the first fully green run on main, including all Windows tests.
- Windows VM is running (VNC 127.0.0.1:5927). The installer from run 37495154837 is installed and verified: per-user install, "Open with Skein" HKCU keys.
- Architecture diagram for the owner: https://claude.ai/artifact/H9LzM3xh2dDJTqoJ2gdBhy (as built and when finished). Source: `.alt/scratch/skein-architecture.html`.
- Mod `handoff-relay` (this warning) lives in `~/.claude-alt/dev-mods/1115865e-…/handoff-relay`.

## In progress (uncommitted or unmerged; nothing here is on main)
| What | Where | State |
|---|---|---|
| Speed fixes, backend | `.alt/perf-backend`, branch `alt/perf-backend` (`99956b6..75457b8`) | Agent may still be running (it had not reported at handoff). Commits: tree fix for big repos (8 MB cap), secret cache per credential revision, async path_identities/probe_root, ordered results, concurrent org listing with partial pages (`warnings: string[]`). **Needs a reviewer pass.** |
| Speed fixes, frontend | `.alt/perf-frontend`, branch `alt/perf-frontend` (`1d8bc12..3ce5566` + review fixes in progress) | Reviewer said "fix first", 9 items. The agent is applying them (contract `warnings[]`, mark stale after fetch/pull, no stuck "Checking…", cancel superseded sweeps, priority upgrade, BranchDialog rows, backoff keep, CRLF). |
| Packet 17 phase 1 (batched Git) | Codex run `crew/api-builder-sfzxd`, `.crew/paperwing-api-builder-sfzxd` | Running since 17:42, uncommitted, base far behind main (conflicts with search's `git/runner.rs` changes). Review the diff and its before/after table; merge only with a clear Windows win. |
| Diagnosis tests | `.alt/check-slow`, branch `check/slow` `b36d112` | Cherry-picked into perf-backend; worktree can be removed after merge. |
| Paused main session worktree | `.claude/worktrees/agent-a488fb7de6d04ca04` (`fix/parallel-tests`, dirty) | Its content is already in main (`3f18795`); safe to remove later. |

## Next steps, in order
1. (Done: run 37498431610 fully green.)
2. Finish the perf-frontend review fixes. Review perf-backend. Merge both into main (the `api.ts` `warnings?: string[]` line must match), run gates, push, and get a new installer.
3. Install the new build on the VM. Smoke-test a set with github.com repos. Tell the owner the artifact link (run page → `skein-windows-nsis`).
4. Packet 17: review the sfzxd result, rebase onto main, then measure on the Windows VM.
5. Write packet 40 (GitHub compare API, GHES-aware, 250-commit/300-file limits with a local fallback).
6. Rename the GitHub repo `trykA123/paperwing` → `skein` and the folder `/mnt/Sabrent/homelab/paperwing` → `skein`. Keep a symlink at the old path, move or link the Claude project memory, and run `git worktree repair`. Do this after the sfzxd run ends.
7. Then the ready packets: 06, 26, 28, 33, 39, 03b, 37+06s.

## Open questions for the owner
- Packet 40: confirm it goes right after the speed work.
- Snapshot storage moves to SQLite (packet 27): fine?
