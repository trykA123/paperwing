# PaperWing → Skein: orchestration plan (2026-10-06)

Orchestrator: Claude Code (Opus 5.5). Codex root thread `01a10f43…` handed over; it keeps only the Windows VM install and writes `windows/vm-20261006/READY.md`.

Base commit: `a4935bf` on `feat/paperwing-linux-completion` (snapshot of accepted 01–14A2).

## Rescoping decisions
- Packet 14 B/C/D: one Linux file service that mirrors the Windows `files.rs` command semantics on the accepted `linux_guard`, `linux_journal` and `linux_diff` libraries. Keep the real invariants (fresh session/generation, root identity, expected bytes, durable backups, partial copy outcomes, page-reload ticket release). Drop the v3 plan's allocation-ledger ceremony (streaming settings parser, per-buffer byte permits, F1 codec mirror). The Codex 14-b foundation worktree is historical only.
- Evidence goes to the worker's own worktree and the commit message, not new JSON ledgers.
- Strict Clippy `dead_code` becomes green as a side effect once Linux write paths are wired.

## Tracks
| Track | Packets | Owner | Status |
|---|---|---|---|
| T1 backend writes | 14 (B/C/D), then 15 | api-builder gpt-6.1-sol xhigh | running |
| T2 metadata | 16 | api-builder gpt-6.1-sol xhigh | running |
| T3 Skein design | Skein steps 1–2 (tokens, Geist, rename, Rails icon) | ui-builder Sonnet 5.5 | running |
| T4 performance | 17 → 18–20 → 21 → 22 | api-builder after T1 | queued |
| T5 Skein layout | Skein steps 3–6 | ui-builder after T2/T3 | queued |
| T6 Windows | VM install (Codex), then 06 + Windows rows of 14/15/23 | Codex, then api-builder | VM install running |
| T7 acceptance | 23, 24 | orchestrator + deployer | last |
| T8 features | 25 open-with (after 15; menus with 24), 26 stash, 27 snapshots, 28 PRs, 29 discard/partial staging, 30 auto-refresh , 31 Actions view (after 28), 33 tags (after Skein step 3), 34 Rust local store (before 28/31/35), 35 Jira read-only + branch from ticket | api-builder + ui-builder | queued, approved 2026-10-06 |

Each branch gets a `reviewer` pass before merging into `feat/paperwing-linux-completion`.

Design spec: `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md` (Skein, Formation, Benzol, Geist + Geist Mono, Rails icon).

Backlog of approved ideas without packets: `.skillify/plans/2026-10-06-skein-backlog/packet.md`.
