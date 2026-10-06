# 06s — Skein step 6: full-screen compare

Status: blocked by the editor decision (pending owner review of the spike report, see 37); layout work can start on the current editor
Platform: Windows first, Linux parity
Size: L
Role: ui-builder

## Goal
A repository compare opens full screen in the style of Beyond Compare. The set summary stays in the app. This packet is linked with 37: 06s owns the layout and chrome, 37 owns the engine and editor.

## Already done
- Views and state: `src/components/SetCompare.svelte`, `FolderCompare.svelte`, `FileCompare.svelte` with `file-compare/Endpoints.svelte`, `Toolbar.svelte`, `Footer.svelte`, `CompareDetails.svelte`, `CopyOperations.svelte`, `DiffViewer.svelte`. State in `src/lib/compare.svelte.ts`, `compare-state.svelte.ts`, `set-compare.svelte.ts`, `compare-view.ts`.
- Compare colours are fixed: names neutral; amber for changed, red for left-only, green for right-only, as markers and edges.
- Skein steps 1 to 5 are merged (tokens, rename and icon, Formation table, History drawer rails, notifications). `HistoryGraph.svelte` and `src/lib/history-graph.ts` draw the two-rail graph.
- Spec: `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md`. Prototype: https://claude.ai/artifact/DDe2HtisyH6CtQEeMjMY5p (`skein-formation.html`).

## Decisions
From the approved spec:
- The set summary stays in the app. A repository compare opens full screen.
- Aligned twin folder trees with filters.
- Copy between sides uses a confirm bar.
- File diff has an overview strip, line details, per-change copy and "Save right".
- A Commits tab shows the two-rail graph.
- Diff colours: added uses ok, removed uses err, changed uses warn. Always add the +/- glyph or a gap stripe.
- Status colours have a graphic tone (at least 3:1) and a text tone (at least 4.5:1), checked in both themes. Fonts are Geist and Geist Mono.
Choices made here:
- "Full screen" means the compare view fills the window below the title bar, with the sidebar and details panel collapsed. Esc or a Back button restores the previous layout. It is not OS full screen.
- The editor sits behind the adapter from 37. Until the owner decides, the layout uses the current Monaco view.
- Compare tabs stay in the tab strip (`app.tabs`), so the user can leave and return.

## Scope
- Do: full-screen layout, folder twin trees, file diff chrome, overview strip, line details, confirm bar, Save right, Commits tab, keyboard map (N and P change navigation).
- Do not: change the compare engine or write paths; choose the editor; build the three-way merge (37).

## Read first
- `~/.claude/handoffs/2026-10-05/skein-design/SPEC.md`
- `src/components/FolderCompare.svelte`, `FileCompare.svelte`, `file-compare/`, `CompareDetails.svelte`, `HistoryGraph.svelte`
- `src/styles/` tokens; `~/.agents/rules/web-ui.md`
- `plans/packets/37-beyond-compare-parity.md`

## Do not touch
- `src/lib/editor.ts` internals and the compare Rust code (37).

## Steps
1. Full-screen shell: collapse sidebar and details, Back and Esc restore. Check: browser test of enter and exit, tab switching, layout restored.
2. Folder twin trees: aligned rows, filters (all, differences, same, orphans), status markers with the colour rules. Check: screenshots with contrast checked by `node ~/.agents/rules/tools/contrast.mjs` for each text and background pair.
3. File diff chrome: overview strip, line details panel, per-change copy, Save right, confirm bar for copies. Check: browser test of copy and save on a fixture.
4. Commits tab with the two-rail graph for the two endpoints. Check: fixture repositories with diverged branches.
5. Keyboard and accessibility pass; reduced motion; 1100 px layout. Check: keyboard-only run; screenshots.
6. Reviewer pass and a Windows VM run.

## Done when
- A repository compare opens full screen at 1440 and 1100 px in both themes, matching the prototype's structure.
- Colour and glyph rules hold in both themes, with contrast numbers recorded.
- Esc restores the previous layout; Save right writes through the platform path.
- Windows: the title bar, window controls and drag region still work in the full-screen view.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- The layout needs an engine change.
- The owner picks an editor that cannot show the overview strip or per-change copy.

## Report
Commit sha, files changed, each step's check result, contrast numbers, screenshots, anything skipped.
