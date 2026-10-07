# Skein plans
One packet per job in `packets/`, written to [`packets/TEMPLATE.md`](packets/TEMPLATE.md). A packet is a complete brief: an agent in a worktree reads it and the files it names, and needs nothing else. Finished packets are deleted; git history keeps them.
Owner priorities: Windows first (work PC, Defender, several GitHub Enterprise hosts plus github.com). Never hardcode github.com. Repositories are the main object; sets are a feature. Deletes are local only. Publish finished UI prototypes as Artifacts.

## In progress
| Packet | What | Where |
|---|---|---|
| [44](packets/44-shell.md) | App shell: module sidebar, details drawer, flyout rail, repo-first | main session, `.alt/ui-44` |
| [38](packets/38-core-boundaries.md) | Core boundaries: event bus, providers per host, provider switch | review, `crew/api-builder-p38-core` |
| [29](packets/29-discard-partial-staging.md) | Partial staging and safe discard (backend) | fixes, `crew/api-builder-p29-stage` |
| [37](packets/37-beyond-compare-parity.md) | Replace Beyond Compare (step 1 CodeMirror done; steps 2-7 next) | `ui/37-editor` |

## Ready now
| Packet | What | Size |
|---|---|---|
| [43](packets/43-runner-hardening.md) | Git runner hardening; one process per Git call on Windows | M |
| [46](packets/46-languages.md) | Colouring for automotive and embedded files (after 37 step 1) | M |
| [18](packets/18-progressive-contract.md) | Progressive results contract (design only) | S |
| [06](packets/06-windows-write-boundaries.md) | Split `files.rs` on Windows, no behaviour change | M |
| [06s](packets/06s-fullscreen-compare.md) | Full-screen compare, with 37 | L |
| [03b](packets/03b-rail-context-menus.md) | Activity rail panels and context menus (after 44) | M |
| [28](packets/28-pull-requests.md) | Pull requests: rail badge left (after 44) | S |
| [30](packets/30-auto-refresh.md) | File watcher refresh (events through 38's bus) | M |
| [40](packets/40-github-compare.md) | Compare on GitHub without cloning (owner to confirm order) | L |
| [24](packets/24-native-artifacts.md) | Native acceptance and honest docs | M |
| [17b](packets/17b-gitoxide.md) | gitoxide reads, after a Windows re-measure (optional) | L |

## Blocked
| Packet | Waits for |
|---|---|
| [31](packets/31-actions.md) Actions runs and logs | 38 |
| [35](packets/35-jira.md) Jira as IssueProvider | 38 |
| [19](packets/19-progressive-backend.md) → [20](packets/20-progressive-frontend.md) progressive results | 18 |
| [21](packets/21-immutable-cache.md) immutable cache | 20 |
| [22](packets/22-prewarm.md) prewarm | 38, 19, 21a |
| [23](packets/23-native-github-acceptance.md) native GitHub acceptance | 20, 21a, 06 |
| [36](packets/36-frontend-performance.md) frontend performance | 20 (step 1 can start) |
| [27](packets/27-release-snapshots.md) release snapshots | 06s |

## Parked
- [32](packets/32-linux-filesystems.md) Linux writes beyond ext4 — Windows first.

## Other
- [backlog.md](packets/backlog.md): approved ideas without a packet.
- Audits still feeding packets: [`2026-10-06/audits/`](2026-10-06/audits/) (architecture items in 43, UX items in 44).
- Owner diagnostics: [`2026-10-07/diagnostics-owner-1.md`](2026-10-07/diagnostics-owner-1.md).
- Design spec: `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md`.
