# Skein plans
One packet per job in `packets/`, written to [`packets/TEMPLATE.md`](packets/TEMPLATE.md). A packet is a complete brief: an agent in a worktree reads it and the files it names, and needs nothing else. Finished packets are deleted; git history keeps them.
Owner priorities: Windows first (work PC, Defender, several GitHub Enterprise hosts plus github.com). Never hardcode github.com. Publish finished UI prototypes as Artifacts.

## Ready now
| Packet | What | Size |
|---|---|---|
| [42](packets/42-backend-hardening.md) | Backend hardening from the architecture audit | M |
| [40](packets/40-github-compare.md) | Compare on GitHub without cloning (GHES-aware) | L |
| [41](packets/41-windows-diagnostics.md) | Anonymous diagnostics export, test builds only | M |
| [38](packets/38-core-boundaries.md) | Core boundaries (34 merged) | L |
| [06](packets/06-windows-write-boundaries.md) | Split `files.rs` on Windows, no behaviour change | M |
| [03b](packets/03b-rail-context-menus.md) | Activity rail panels and context menus | M |
| [28](packets/28-pull-requests.md) | Pull request column, open dialog, bulk open | M |
| [29](packets/29-discard-partial-staging.md) | Discard changes and partial staging (backend first) | M |
| [30](packets/30-auto-refresh.md) | File watcher refresh (timed fetch waits for 19) | M |
| [24](packets/24-native-artifacts.md) | Native acceptance and honest docs (packaging done) | M |
| [37](packets/37-beyond-compare-parity.md) | Replace Beyond Compare: CodeMirror 6 + large-file renderer | L |
| [06s](packets/06s-fullscreen-compare.md) | Full-screen (OS) compare, with 37 | L |

## In progress
| Packet | Where |
|---|---|
| [17](packets/17-cold-path.md) | Phase 1, Codex run in `.crew/paperwing-api-builder-sfzxd` |

## Blocked
| Packet | Waits for |
|---|---|
| [31](packets/31-actions.md) CI runs, GitHub Actions first | 38 |
| [35](packets/35-jira.md) Jira as IssueProvider | 38 |
| [17b](packets/17b-gitoxide.md) gitoxide reads | 17 |
| [18](packets/18-progressive-contract.md) → [19](packets/19-progressive-backend.md) → [20](packets/20-progressive-frontend.md) progressive results | 17 |
| [21](packets/21-immutable-cache.md) immutable cache | 17, 20 |
| [22](packets/22-prewarm.md) prewarm, Bank A/B/C | 38, 19, 21a |
| [23](packets/23-native-github-acceptance.md) native GitHub acceptance | 17, 20, 21a, 06 |
| [36](packets/36-frontend-performance.md) frontend performance | 20 (step 1 can start) |
| [27](packets/27-release-snapshots.md) release snapshots | 06s |

## Parked
- [32](packets/32-linux-filesystems.md) Linux writes beyond ext4 — Windows first.

## Other
- [backlog.md](packets/backlog.md): approved ideas without a packet.
- Design spec: `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md`.
- Dated folders (`plans/2026-10-06/`) hold handoffs and orchestration notes.
