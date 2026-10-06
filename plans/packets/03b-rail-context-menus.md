# 03b — Activity rail panels and context menus

Status: ready (rail shell is merged); the Pull requests, Changes and Actions panels arrive with 28, 29 and 31
Platform: both
Size: M
Role: ui-builder

## Goal
Every common action, including compare, is reachable by right-click or keyboard from the repository table, the ref pickers and the compare views. The rail panel for Compare lets the user start any comparison from one place.

## Already done
- Rail shell: `src/components/ActivityRail.svelte` and `src/lib/rail.ts` (`RAIL_SECTIONS`: sets, compare, recovery, activity; Ctrl+1..4, Ctrl+5 opens Settings). Badges today: open comparisons and Git activity.
- Panels: `src/components/panel/ComparePanel.svelte`, `ActivityPanel.svelte`, `RecoveryEntry.svelte`.
- One hand-written row menu: `src/components/set/RowMenu.svelte` (History, Show details, Compare, Git commands, Rename, Duplicate, Remove). It has Escape, Tab and arrow handling, but no Shift+F10 or Menu key and no typeahead.
- Commands live in `src/lib/commands.ts`. Set compare opens through `app.openSetCompare()`, repository compare through `app.openCompare(item, true)`.
- Missing: a shared ContextMenu component; menus on ref pickers, history drawer and compare rows; "Compare these two"; badge sources beyond comparisons and activity.

## Decisions
- One reusable `ContextMenu.svelte`. `RowMenu.svelte` becomes a thin caller of it.
- Menu items call existing commands only. The menu adds no new mutation path.
- Items for features that are not built yet (Stash, Tag, PR) are added by 26, 33 and 28, not here.
- Badges: pull requests to review (28), failed runs (31), unresolved recovery records (now).

## Scope
- Do: ContextMenu component with keyboard support; menus on the surfaces below; Compare panel "New compare" and recents; recovery badge.
- Do not: new backend commands, new mutations, new rail sections.

## Read first
- `src/components/set/RowMenu.svelte`, `src/components/panel/ComparePanel.svelte`, `src/lib/rail.ts`, `src/lib/commands.ts`
- `src/components/RefPicker.svelte`, `src/components/HistoryDrawer.svelte`, `src/components/FolderCompare.svelte`
- `src/styles/` tokens; `~/.agents/rules/web-ui.md`

## Do not touch
- Compare engine and file-write paths (`src/components/FileCompare.svelte`, 37 and 06s own them).

## Steps
1. Build `ContextMenu.svelte`: Shift+F10 and the Menu key open it, arrows move, typeahead jumps, Escape closes and returns focus, viewport clamping. Move `RowMenu` onto it. Check: a Vitest-style unit test of the keyboard model in `src/lib`, plus a browser check.
2. Repository rows: add "Compare with…" (another repository, origin, branch, tag, snapshot when 27 lands), "Open in terminal", "Copy path". With two rows selected, show "Compare these two". Check: browser test opens the compare view from each item.
3. Ref picker and history drawer: branches and tags offer "Compare with current", "Check out", "Tag here" (Tag only once 33 UI exists; hide it, do not disable it). Check: browser test.
4. Folder and file compare rows: "Copy to left", "Copy to right", "Open", "History", wired to the existing copy and open commands. Check: copy still goes through the confirm bar.
5. Compare panel: "New compare" picks any two of repository, branch, tag, commit, folder, snapshot; recent and pinned comparisons; set-wide compare. Check: browser test.
6. Rail badges: unresolved recovery count now; other badges register through one map so 28 and 31 add theirs without editing the rail.

## Done when
- Every listed menu works by mouse and keyboard at 1440 and 390 px in both themes.
- "Compare these two" opens the compare view.
- Windows: the Menu key and Shift+F10 work in the packaged app; no menu item bypasses the Windows write path.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (`--write` regenerates `src/styles/order.json`)
- UI: screenshots at 1440 and 1100 px, both themes, plus 390 px for menus

## Stop and report if
- A menu item needs a command that does not exist.
- Focus handling conflicts with the Tauri title bar drag region on Windows.

## Report
Commit sha, files changed, each step's check result, gate results, screenshots, anything skipped.
