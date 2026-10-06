# Skein handoff — 2026-10-06, 18:10 Europe/Bucharest

Orchestrator: Claude Code (Opus 5.5). Read this first, then `plans/2026-10-06/orchestration.md` and memory (`~/.claude/projects/-mnt-Sabrent-homelab-paperwing/memory/MEMORY.md`).

## Owner priorities
- Windows `.exe` is the main target (work use, Defender on). Linux matters less for now.
- Publish every finished UI prototype as an Artifact and send the link.
- Owner is often on the phone; they approved running commands and pushing the feature branch (2026-10-06).

## Branch state
`feat/paperwing-linux-completion` at `89df8ed`, pushed to `origin` (public repo `trykA123/paperwing`). CI run `37481629860` (workflow `build`: Linux + Windows tests, NSIS `Skein.exe`, deb/AppImage) was in progress at handoff. Uncommitted in the main checkout: only the owner's two deleted `.skillify/plans/2026-10-01-flock-*` files (leave them) and `docs/improvements.md` (owner file, untracked).

Merged so far: packets 01–05, 07, 09–16, 25 (app side), 26 backend (stash), 33 backend (tags), 28 client (pull requests, incl. GHES), branch cleanup, set search, Skein steps 1–5 + compare colour fix, Windows packaging/NSIS "Open with Skein" hooks, custom Windows title bar, CI.

Fixed today on Windows: `valid_path` checked the bare drive prefix `C:` (= the drive's current directory), so launching from a junction/OneDrive folder broke every repo ("Refs unavailable"). Fix `5f455dc`, regression test passes on the VM. Not yet confirmed against the owner's machine — ask them to try the new CI build and hover the label if it persists.

## In flight
| Work | Where | Status |
|---|---|---|
| Packet 17 phase 1 (batched Git, per-repo readers, in-process blob hashing, raw binary IPC) | crew `api-builder-sfzxd`, `/mnt/Sabrent/homelab/.crew/paperwing-api-builder-sfzxd` | Running (third run). Decisions are in its brief: byte-identical results to today, per-repository readers, fast path only within one repo. Prior controls in `.crew/paperwing-api-builder-rzs29`. Must deliver a before/after table; then re-measure on the Windows VM. |
| Parallel-flaky Rust tests | Claude agent, worktree branch `fix/parallel-tests` from `a66b640` | Running. Tests pass alone but fail together (shared Git runner / probe semaphore / budget). Fix = take existing `TEST_RUNNER_LOCK` / `test_support` locks. Also `platform::tests::abandoned_root_probes_hold_the_four_task_limit_until_blocking_cleanup`. |
| Editor spike (Monaco vs CodeMirror 6 merge view) | Claude agent, scratch `editor-spike/` (or `/mnt/Sabrent/homelab/.crew/editor-spike/`) | Running. Result feeds packet 37 (replace Beyond Compare) and Skein step 6. Publish its report page as an Artifact. |
| Packet 34 SQLite store | branch `feat/local-store` `e979818` (worktree `.claude/worktrees/agent-a62745075d9abd241`) | Done, NOT reviewed, NOT merged. Run `reviewer`, then merge. |

## Decisions taken today
- Packet 14 rescoped: Linux file service mirrors Windows `files.rs` on the accepted linux_* libraries (no allocation-ledger ceremony). Merged.
- Linux writes only on ext4 for now; packet 32 adds btrfs/xfs/f2fs (full) and NTFS/FAT32/exFAT (reduced). Parked (Windows first).
- Skein design: Formation layout, Benzol palette, Geist + Geist Mono, Rails icon, activity rail on the left (Sets, Compare, Recovery, Activity, Settings). Only Geist/System fonts offered now.
- Compare colours: names neutral; amber ≠ changed, red left-only, green right-only, as markers and edges.
- New packets (plans in `.skillify/plans/2026-10-06-skein-*`): 25 open-with, 26 stash, 27 snapshots, 28 PRs, 29 discard/partial staging, 30 auto-refresh, 31 Actions view, 32 Linux filesystems, 33 tags, 34 local store, 35 Jira read-only + branch from ticket, 36 frontend performance, 37 Beyond Compare parity, step 3b rail sections + context menus; backlog file lists the rest.

## Waiting on the owner
- Pick the Jira layout from https://claude.ai/artifact/5aK3CYYU1YeYid9SxWbtQC (designer recommends C, the work hub).
- Try the new `Skein.exe` from CI run `37481629860` once green (artifacts on the run page).
- Whether to bring back other font choices (default answer: no).

## Next steps (in order)
1. Watch CI `37481629860`; fix any Windows build/test failure.
2. Merge `fix/parallel-tests` when green; make CI fully green.
3. Review + merge packet 34.
4. Land packet 17 phase 1 with the before/after table (Linux + Windows VM).
5. Wire UIs for tags (33), stash (26), branch cleanup, search, PR status (28) into the Formation table and rail; step 3b context menus.
6. Skein step 6 full-screen compare with the editor chosen by the spike (packet 37).
7. Packets 18–20 progressive results, 21 cache, 36 frontend performance; then 35 Jira, 31 Actions, 27, 29, 30.
8. Packets 23/24 acceptance on the Windows VM; packet 06 Windows write boundaries.

## Environment and traps
- Windows VM: `.skillify/evidence/paperwing/windows/vm-20261006/READY.md`. `ssh -F ~/.config/paperwing-windows/ssh_config paperwing-windows` (PowerShell 7). Start `./start.sh`, stop with `Stop-Computer -Force` over ssh then `./stop.sh`. Snapshot `provisioned`. Admin password rotated 16:51; only in `~/.config/paperwing-vm/.env` — never print it. Guest has `C:\Users\admin\src\skein` and a test junction `C:\Users\admin\repos\junc`.
- `/tmp` is a 16 GB tmpfs: keep Cargo target dirs and big fixtures under `/mnt/Sabrent/homelab/.crew/` (ext4). It filled up once today and killed agents.
- Linux write tests need an ext4 root: `src-tauri/src/test_support.rs` `tmp_root()` / `PAPERWING_TEST_TMP`.
- Codex (`crew`) sandbox: Bun/esbuild fails ("The service was stopped") and ACL/namespace tests fail inside it; re-run frontend gates and those tests outside before trusting a report. Codex quota hit 98% once; check before large runs.
- Claude subagent effort comes only from `~/.claude/agents/*.md` frontmatter (`ui-builder`/`designer` medium, `ui-builder-high` for hard work).
- Merging: `css-order.ts --write` regenerates `src/styles/order.json`. Line-union conflict resolution is only safe for lists (handler registrations, api.ts entries, CHANGELOG); it broke `github/http.rs` once — resolve code by hand.
- A deploy-gate hook blocks `git push` until `bun run check` passes on the current code.
- Second Claude profile for another account: `claude-alt` (config `~/.claude-alt`, shares agents/skills/memory). Its handoff is `plans/2026-10-06/parallel-handoff.md`; claims in `plans/2026-10-06/parallel-claims.md` (untracked). Its work is fully merged.
- The old Codex root thread `01a10f43…` is idle; Codex's 14B foundation worktree `.skillify/evidence/paperwing/worktrees/14-b-recovery-20261005` is historical only.
