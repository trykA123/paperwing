# 44 — App shell: module sidebar, details drawer, flyout rail

Status: ready (owner pick 2026-10-07: direction A + rail R3)
Platform: both; Windows first (1440 and 1100 window sizes)
Size: L
Role: ui-builder-high (frontend only), one writer; split into the steps below

## Goal
Every module (Sets, Changes, Branches & tags, Compare, Search, Pull requests, Actions, Jira, Activity, Recovery, Settings) has a place. The repository table keeps today's look and gains about 380 px of width. Hosted modules group under their provider in a flyout rail. Details for any selected item open in one drawer.

## Source of truth
- Prototype (alt session, Opus): `/mnt/Sabrent/homelab/.alt/shell-design-2/a-final.html`, with sources in `src/afinal*.{html,js,css}`. Use direction A and rail R3.
- Screenshots: `/mnt/Sabrent/homelab/.alt/shell-design-2/shots/final/` (`rail/R3-*`, `sets-popover-*`, `prs-scope-*`, `actions-list-*`, `run-*`, `job-*`, `placeholder-*`). Layout metrics: `shots/final/metrics.json`.
- Owner artifacts: round 2 `https://claude.ai/artifact/4AfeSypJYXLBz5h4pVLwW5`, final `https://claude.ai/artifact/A2ew6DqdBkVzNLMKnMxW1m`. Both are owned by the alt account; use the local copy above.
- Owner constraints: today's repository look is the baseline (comfortable rows, sync rails, branch chips, next-action buttons). Compact stays an option. Never make the table narrower.

## Already done
- Rail shell: `src/components/ActivityRail.svelte`, `src/lib/rail.ts` (`RAIL_SECTIONS`, Ctrl+1..5).
- Panels: `src/components/SidePanel.svelte`, `Sidebar.svelte`, `panel/*`.
- Right panel: `RightPanel.svelte`, `right/SetSummary.svelte`, `right/CloneFooter.svelte`.
- History drawer: `HistoryDrawer.svelte`, which already hosts the stash and tag sections.
- Module data that exists: pull requests (`src/lib/pulls.svelte.ts`, seam `pulls.awaitingReview`), code search view (`codeSearch` tab), stash and tags flows, branch cleanup, diagnostics.
- Not built yet, so render placeholder pages in the shared frame: Changes (29), Actions (31), Jira (35). Branches & tags gets a list page built from existing flows.

## Decisions (final)
- **Rail (R3).** Groups: Local Git (Sets, Changes, Branches & tags, Compare, Search), Providers, System (Activity, Recovery, Settings).
  - Each enabled provider gets one button with a summed badge; the badge turns red when a run fails. A disabled provider's button disappears.
  - Clicking or hovering a provider button opens `RailFlyout.svelte` (role `menu`). It has one section per host (github.com, each GHES host), listing Pull requests, Actions and Releases with counts.
  - Hover opens after 150 ms, with a safe pointer path. Keyboard: Enter, Space and ArrowRight open it; arrows and Home/End move; typeahead; Esc and ArrowLeft close and return focus to the rail button.
  - Ctrl+1..5 go to the Local Git modules. Ctrl+J opens Activity. Ctrl+, opens Settings.
- **Module sidebar.** The 250 px sidebar shows the current module's own lists:
  - Sets: today's sets list, favorites, browse.
  - Pull requests: queues (awaiting my review, created by me, assigned, drafts, all open).
  - Actions: workflows.
  - Jira: filters.
  - Search: mode and recent searches.
  - Branches & tags: cleanup, tags, stash.
- **Main area.** One page frame for every module: crumb, title and subtitle, header buttons, filter chips, a table with 56 px comfortable rows, and the pager with the density toggle. Reuse `formation.css` rows.
- **Right panel removed.** Drop the `right` grid track.
  - Details open in one generalised drawer: `HistoryDrawer.svelte` becomes `DetailsDrawer.svelte`, with sections per entity. A name click or Enter opens it.
  - Destination and the clone plan move to a set-header popover (`sets-popover-*` screenshots). It reuses `right/CloneFooter.svelte`; the plan list is capped at 8 plus "and N more".
- **Scope.** Pull requests and Actions are scoped to the active set by default. A chip ("In <set> ⇄") widens the scope to all repositories. The scope persists per module in the workspace.
- **Widths.** The repository table must be at least 1094 px at a 1440 window and 770 px at 1100 (metrics.json). There is no horizontal scroll on the Sets table at either width in comfortable density.
- **Actions run page.** Packet 31 owns it (GitHub-style run page: jobs in the module sidebar, steps and logs in the main area). This packet only provides the frame and a placeholder.
- **Workspace migration.** Saved workspaces with `rightVisible` or a right width still load. Those fields are ignored, and nothing else is lost.

## Scope
- Do:
  - rail groups and flyout
  - module registry: id, label, icon, group, provider, `badge()`, sidebar component, page component
  - one sidebar panel per module
  - the shared page frame
  - DetailsDrawer
  - the set-header destination popover
  - scope chips
  - removing the right panel
  - workspace migration
  - placeholders for 29, 31 and 35
  - wiring for the existing PR, search, stash, tags and cleanup UIs
- Do not:
  - backend changes
  - building Changes, Actions or Jira content
  - restyling the repository rows
  - changing compare internals (packet 37)

## Read first
1. Prototype files and screenshots above, plus `plans/2026-10-07/shell-redesign-brief.md`.
2. `src/App.svelte`, `src/styles/shell.css`, `ActivityRail.svelte`, `rail.ts`, `SidePanel.svelte`, `Sidebar.svelte`, `RightPanel.svelte`, `right/*`, `HistoryDrawer.svelte`, `SetHeader.svelte`, `src/lib/workspace.ts`, `src/lib/state.svelte.ts`.
3. `plans/2026-10-06/audits/ux.md` (UX-01, 12, 13, 17, 18, 24, 25), `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md`, `~/.agents/rules/web-ui.md`, `typescript.md`, `code-quality.md`.

## Steps (each one a reviewable commit)
1. **Module registry.** Add `src/lib/modules.ts` and drive the rail from it, including groups, provider visibility, badges and shortcuts. Check: unit tests for visibility and badge sums; the rail renders identically for Sets.
2. **RailFlyout.svelte.** Hover delay, safe path and full keyboard support. Check: a browser test of the keyboard and hover paths, and of two hosts.
3. **Module sidebar and page frame.** Build one panel per module. Pull requests and Search move from tabs or lists into the module (the PR queue page uses `pulls`). Check: browser screenshots per module at 1440 and 1100, both themes.
4. **DetailsDrawer.** Generalise `HistoryDrawer` with sections per entity (repository, PR, search hit, tag, stash). Remove `RightPanel` and the right grid track. Add the destination popover in `SetHeader`. Check: the table width meets the targets (measure in the browser); the clone flow still works from the popover.
5. **Scope chips and workspace migration.** Check: unit tests for scope persistence and for old workspaces loading.
6. **Placeholders** for Changes, Actions and Jira in the shared frame. The PR rail badge reads from `pulls.awaitingReview`. Check: screenshots.

## Done when
- Every module is reachable by mouse and keyboard.
- Disabled providers disappear from the rail.
- The Sets table at 1440 and 1100 meets the width targets with no horizontal scroll and today's row look.
- Nothing that the right panel showed is lost: the destination, the clone plan and the set summary all have a home.
- Windows: the custom title bar and the drag region still work. Flyout and drawer stay above the WebView's native elements.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- Browser: 1440 and 1100, both themes, both densities. Compare against the prototype screenshots.

## Stop and report if
- A width target cannot be met without restyling rows.
- Removing the right panel loses a function that has no home in the drawer or the popover.

## Repo-first (owner pick 2026-10-07: direction 1, contextual sidebar)
Prototype: `/mnt/Sabrent/homelab/.alt/repo-first/index.html` (direction 1) and its screenshots in `shots/A-*`. Artifact: https://claude.ai/artifact/Gzr3YKgSVKCnhSf2hUEX3m.
- **Repositories module.** It is home and Ctrl+1. It lists all known repositories: cloned ones plus remote-only ones from configured orgs.
  - Filter chips: All, Cloned, Has changes, Behind, Favorites.
  - A set chip ("Any set ▾") filters to a set and shows a set bar: Fetch, Pull, Push, Clone, Compare set, Edit, Delete. Deleting a set keeps its repositories.
  - Host and org filters live in the sidebar tree. Remote-only rows show "remote" and a Clone next action.
  - Sets leave the rail. They become a sub-state of Repositories. Create a set from the sidebar or from the bulk bar ("Add to set").
- **Repository page.** A name click, a favorite, Enter on a row, or the drawer's "Open repository page" opens it.
  - The sidebar turns into that repository's sections: Overview, Changes, History, Branches & tags, Stash, Pull requests, Actions, Compare. Each shows a count. Sections that need a clone are disabled with "Needs a clone". Favorites stay below.
  - The main area shows one section at a time. Overview has tiles plus small cards.
  - Header: crumbs (Repositories › host › org), star, cloned or remote-only tag, path, branch picker, sync, the next action as the primary button, Fetch, "Open in ▾" and more.
  - Back: the sidebar back button, the crumb, or Alt+Left. Back restores filters and scroll.
- **Drawer.** A row click or Space opens the quick look: status, next action, 3 changes, mini history, PRs and failing runs, plus "Open repository page". Inside a page, the same drawer shows a commit, PR, run or stash.
- **Deletes.** Every delete confirmation states "Local only. Nothing on <host> changes."
- **Code.** In `modules.ts`, a `repos` module replaces `sets` as home. Favorites (`Sidebar.svelte:58`) call `openRepository(id)`; set membership moves to the star and row menu. New `RepositoryPage.svelte` and `RepositorySidebar.svelte`, routed through a `repo` view kind in `workspace.ts`, which stores the last repository and section. `SetHeader.svelte` becomes the set bar. `HistoryDrawer.svelte` becomes `DetailsDrawer.svelte`. Reuse `HistoryGraph`, the stash, tags and pulls flows, and the local-only delete flows.
