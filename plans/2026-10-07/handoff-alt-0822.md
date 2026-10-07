# Skein handoff, 2026-10-07 08:22 (Europe/Bucharest), from the alt session (paperwing-c9, ~/.claude-alt)

Read this first, then the running log in `plans/2026-10-06/handoff-main.md` (from 22:57 on, the alt entries) and `plans/2026-10-06/parallel-claims.md`. The alt session took over merge and push from main (owner: "take it from him").

## Owner decisions today
- Shell: **direction A (module sidebar + drawer) with the R3 flyout rail**. Prototypes: round 2 https://claude.ai/artifact/4AfeSypJYXLBz5h4pVLwW5, A final https://claude.ai/artifact/A2ew6DqdBkVzNLMKnMxW1m (source `.alt/shell-design-2/`). Add-ons: Actions and PRs scoped to the active set with a chip to widen; Destination and clone plan in a set-header popover; empty states centred in their card.
- Actions run page mirrors GitHub's: jobs in the module sidebar, summary with job graph, annotations, artifacts, steps that expand to logs. Written into packet 31 (`3c983db`). No live log streaming (no public API); step summaries link to GitHub.
- **Repo-first**: "sets of repos is a feature not the main event now". Favorites must open a repo. Today a favorite click only toggles set membership (`Sidebar.svelte:58`) and there is no repo page.
- **Deletes are LOCAL ONLY** ("EXTREMLY IMPORTANT, I DON'T WANT TO DELETE FROM REMOTE"). Delete set is verified local (settings edit + optional Recycle Bin via SHFileOperation FOF_ALLOWUNDO).
- Owner rule kept: if a fix fails review twice, do not hold the build.
- Designer may run on Opus only when the owner asks (it did "this once" for round 2).

## Done (on main and pushed)
- Pushed main 3c983db (51 commits: 39, 41, 26, 28, 33, polish A/B), then:
  - ed6944a CI test fix: SKEIN_TEST_TMP fallback via `test_support::tmp_root`, `local/slow_tests` unix-only.
  - 76c341e skip the `*` gitlink test on Windows.
  - bf3719f **diag build fix**: `VITE_SKEIN_BENCHMARK` only records UI timings; harness mode needs `VITE_SKEIN_BENCHMARK_FIXTURE` (tauri.test.conf.json). This fixed "Command benchmark_plan not found" and Sync stuck on "Checking…" in the diag build.
- CI run 37573417043 (bf3719f) is green after a rerun of a flaky Linux process test. Installers are on **attempt 1**: https://github.com/trykA123/paperwing/actions/runs/37573417043/attempts/1 (rerun attempts hide artifacts; push a new commit instead of rerunning next time).
- Local main, not pushed: dc5bcb9 (backlog: flaky process test), b6ffb26 (owner diagnostics findings).

## In progress
| What | Where | State |
|---|---|---|
| Packets 42 + 17 + Next-action fix | `merge/final` in `.alt/merge-final`, tip fa5fcdd (main + 42 d4dcc9a + 17 849c8c7 + sha2 dedup 283d541 + ui/next-action) | Frontend gates green (check 0, bun 360, build, css-order). Cargo test + clippy were running at handoff (task started 08:19). If green: `git -C paperwing merge --ff-only merge/final`, run `bun run check` alone in the main checkout (push gate hook), push, watch CI, give the owner the run link. |
| Repo-first designs | designer, output `.alt/shell-design-2/repo-first.html`, shots in `shots/repo-first/` | Running. Check shots, publish as Artifact, post link. |

Packet 42 final state: rounds 4-6 in `crew/api-builder-mxk9r` (b965e6f, a12fb61, af50369). Close always works; no saves while settings failed to load; Retry action; internal loads never touch recovery state; `.bak` guard; 120 s stash snapshot. Last review's only blocker is fixed by af50369 (test fails before the fix); no review ran after af50369.
Packet 17 final state: `crew/api-builder-lywvg` (8e84c81 round 3, 92ed04e Git `convert_is_binary` model). Review: approve-with-nits. Benchmark: CRLF working tree 13 s -> 0.66 s on 20k files, all fingerprints match (`src-tauri/target/packet17-round3/medians.json` in that worktree).

## Waiting on the owner
1. Remove remote branch delete (packet 39 branch cleanup) and remote tag delete (packet 33)? Recommended: yes, given the local-only rule.
2. Code search: what did the screen show, and did they expect a search over all ~845 GitHub repos (GitHub code search API) instead of the cloned repos in the set? The checker found no defect on Linux (46 search tests pass; no flag newer than Git 2.37; failed jobs always emit search-done).
3. Pick a repo-first direction once published.
4. Update Git for Windows (2.37.0): `winget upgrade --id Git.Git -e --source winget`. Then a second diagnostics export.

## Next steps, in order
1. Finish the merge above and push; watch CI.
2. Publish repo-first designs; on the pick, write the shell packet (A + R3 + repo-first), absorbing UX-01, 12, 13, 17, 18, 24, 25, packet 03b and the packet 28 rail badge.
3. If the owner says yes: packet to remove the remote delete paths, with tests that no remote-delete command remains.
4. Diagnostics follow-ups (`plans/2026-10-07/diagnostics-owner-1.md`): every Git spawn costs 170-320 ms on the owner's machine; status averages 1.5 s. Next targets: fewer status calls, `--untracked-files=no` for row status, batch where possible. Add `search-code` to `benchmark::OPERATIONS` so grep shows in exports.
5. Packet 17 nits: 128-printable boundary and mid-file ^Z unit tests; backlog line for revoke-and-restart readers is in.
6. Packet 42 nits: FolderCompare/SetCompare saves fail during recovery (pre-existing); app-lifecycle test helper is brittle (filters timers by 400 ms).

## Cleanup when safe
- Alt-created worktrees: `.alt/merge-final` (keep until pushed), `.alt/shell-2-base`, `.alt/ui-next-action`, `.alt/check-search` (has an untracked git 2.37 clone in `.scratch-tmp/git237`; delete it).
- Finished crew worktrees: 24d6y, 1ci2m, k5ts0, j6wme, lmu2p, lvjtv, lws9p, ly653, mxk9r, lywvg (after merge).
- Port 41400 is held by a bun process not started by alt (PID 2442924); leave it.

## Notes
- Codex usage limit was hit at about 07:50; it resets 11:30. Alt finished rounds inline since then.
- The push gate hook wants `bun run check` run alone in the main checkout after the last change.
- `sub.py` helper for CRLF-safe edits: `/tmp/claude-1000/-mnt-Sabrent-homelab-paperwing/6b413f7d-479e-4041-bd20-aeacd59ef039/scratchpad/sub.py` (session scratch; recreate if gone).
