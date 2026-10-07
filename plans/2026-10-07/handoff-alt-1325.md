# Handoff from alt (paperwing-c9) to main, 2026-10-07 ~13:25 Europe/Bucharest

Alt is at 97% usage. Main takes everything below. Claims: `plans/2026-10-06/parallel-claims.md`.

## Running now (Codex sol, started directly with `codex exec`, NOT through crew, so no crew message arrives)
Check each with `cat ~/.cache/crew/api-builder-<id>.exit` (exists when done) and `tail -60 ~/.cache/crew/api-builder-<id>.last`. Relaunch with `bash ~/.cache/crew/api-builder-<id>.sh` after editing the `.prompt`. Workers do not commit; verify the diff, check line endings (`file`), run the gates outside the sandbox, commit on the branch, then review.

| id | Worktree / branch | Task |
|---|---|---|
| `p38-core` | `.crew/paperwing-api-builder-p38-core`, `crew/api-builder-p38-core` (commits e4c6fa0, f4e2905) | Packet 38, round 2. Decisions: frontend events are emitted synchronously (broadcast only for in-process); settings.json is the single source of truth for `Source.enabled` (no SQLite read in the settings load path); typed payloads byte-identical; jobs cancel on emit failure; module `core` renamed to `kernel`; accessible switch in Settings. Then: reviewer, merge. |
| `p29-stage` | `.crew/paperwing-api-builder-p29-stage`, `crew/api-builder-p29-stage` (commit 7eb5604) | Packet 29 backend, round 2. Decisions: use Git's own conversion (`hash-object --path`, `cat-file --filters`) for autocrlf/eol; refuse `filter` attribute (LFS) files; refuse a selection that splits a no-final-newline line; partial unstage keeps a rename; intent-to-add is untracked; 3-line context; index check per path entry. Windows untracked discard stays REFUSED (SHFileOperation can delete permanently). Then: reviewer, merge. |
| `rename` | `.alt/cleanup`, `chore/skein-cleanup` (commit 20d80ba has the plans/docs pruning) | Remove every "paperwing" name, clean break: identifier `dev.skein.app`, test `dev.skein.testing`, keyring service `skein`, `.skein-` temp prefixes (stage-prefix length check derived from a const), no `PAPERWING_` env fallbacks, no legacy settings import. Then: gates, commit on the branch. |

## Ready to merge
- **Packet 37 step 1** on `ui/37-editor` (`.alt/ui-37`; commits 80d049d, 3c1cab0, ac81bfe). Reviewed; fixes done; check 0, bun 482, build OK. It changes package.json/bun.lock (adds @codemirror/*, removes monaco-editor). Merge into main, run `bun install` in the main checkout (shared node_modules) when ui-44 is idle, then rebase or merge ui-44 onto main.

## Merge order
37 → 38 → 29 → chore/skein-cleanup.
- 38 vs 29: lib.rs conflicts. Move 29's five commands (`change_hunks`, `stage_hunks`, `unstage_hunks`, `discard_files`, `discard_hunk`) into `src-tauri/src/commands/changes.rs` (38's domain registration). The inventory should then be 87 names / 107 registrations.
- After the cleanup merges, run `git grep -i paperwing` again: 29/38 may have added names.
- Push only after all gates pass; Windows proof is CI `test-windows`.

## Owner decisions today (alt side)
- Windows test VM DELETED (container and 65 GB disk). Windows proof: CI plus the owner's work PC.
- `.skillify` removed (f953d20, on main). Backups: `homelab/backups/skein-cleanup-20261007` (old 14-b worktree patch, small evidence tgz).
- Rename to Skein everywhere, clean break. On the work PC: uninstall the old Skein, install the new one, re-add sources, tokens and sets.
- Repo rename: GitHub `trykA123/paperwing` → `skein` (`gh repo rename skein`, then `git remote set-url origin https://github.com/trykA123/skein.git`). Then the local folder `/mnt/Sabrent/homelab/paperwing` → `/mnt/Sabrent/homelab/skein`, LAST, when no session or agent runs there. Move every worktree with `git worktree repair` after the move, and move the Claude memory dirs (`~/.claude*/projects/-mnt-Sabrent-homelab-paperwing`) to the new path name.
- Plans keep only work left to do; finished packets are deleted. New packet 43 (runner hardening; the Git for Windows launcher doubles every spawn).
- Packet 46 is retargeted to CodeMirror and runs after 37.
- Code search works (the owner had searched the wrong term). GitHub-wide search is packet 45 (main's).
- Deletes are local only; remote branch/tag delete was removed earlier by main.

## For packet 44 (main)
- Disabled-source styling: Sidebar, org view, set rows and PR column show "Disabled", hide counts, disable Refresh, and show neutral text instead of retrying errors.
- Empty states centred; repo-first direction 1; R3 flyout.

## Owner side requests still open
- Cameras at the parents' home (homelab `homeassitant/`): Hikvision DS-7108HGHI-F1/N (8 ch Turbo HD), ~50 Mbps upload. A mini PC will come; a hybrid XVR or IP cameras later. Proposed: mini PC with Tailscale (subnet router) plus Frigate (OpenVINO) and go2rtc, and HA connecting over Tailscale (Frigate integration, a "Parents" dashboard, person alerts). RTSP `/Streaming/Channels/<n>01` for main and `<n>02` for substreams. The owner has not said yes to preparing the kit yet; ask.

## Known facts
- Codex sandbox: no git writes, esbuild fails; rerun bun check and build outside.
- The push gate hook wants `bun run check` run on its own in the main checkout before `git push`.
- crew's usage check reported sol at 98% while Codex worked; launching `codex exec` directly with crew's flags worked.
