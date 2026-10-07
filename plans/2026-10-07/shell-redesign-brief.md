# Shell redesign, round 2: brief for the alt session (designer on Opus)

Owner request, 2026-10-07: "I don't like any of the directions, it's too compacted, I like the way repos look now. Have the other alt-session run with Opus to do another prototypes."

## Owner feedback on round 1
- Rejected: all four directions in https://claude.ai/artifact/XcvJfgqVBQps3egAJ4waZG (source `.alt/shell-design/index.html`).
- Reason: too compact.
- Keep: the current repository view as it is today on main. That means the Formation table with comfortable rows, the sync rails, the branch chips, the next-action buttons, the set header with filter chips, and the bulk bar.

## Goal (unchanged from round 1)
Make room for modules: Sets/Repositories, Changes, Compare, Search, Branches & tags (stash, cleanup, tags), Pull requests, Actions/CI, Jira, Activity, Recovery, Settings. Disabled providers' modules disappear. The main window should suit each module.

## Constraints for round 2
- Start from the current app, not from a new visual system. Run main's frontend with the browser harness (`scripts/testing/browser/`, plus the 800-repo mock at `.alt/ux-audit/harness/`) and screenshot today's look first.
- Repositories stay in today's density and styling. Comfortable is the default; compact stays an option, not the baseline.
- Add modules around that view. Do not shrink it. Show how the rail, a module sidebar, or tabs grow without taking width from the repo table at 1440 and 1100.
- Every other module (PR queue, Actions, Jira, Search, Compare) should feel like a sibling of today's repo view: same row height, spacing, type and chips.
- Use the approved design: `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md` (Skein, Formation, Benzol, Geist and Geist Mono, Rails icon).
- 3 or 4 directions behind one picker. Include light and dark, 1440 and 1100, fake data only (user `admin`), and notes per direction (idea, strengths, trade-offs, what changes in the code).
- Run the designer on Opus. The owner asked for this explicitly.
- Publish as an Artifact and post the link in `plans/2026-10-06/parallel-claims.md`.

## Context
- The UX audit (`plans/2026-10-06/audits/ux.md`) items UX-01, 12, 13, 17, 18, 24 and 25 wait for this pick.
- Packet 28's rail badge waits on it (seam `pulls.awaitingReview`).
- The search follow-ups wait on it too (file-name, multi-ref and history search; see `plans/packets/backlog.md`).
- The main session (paperwing-53) does not touch the shell layout until the owner picks.
