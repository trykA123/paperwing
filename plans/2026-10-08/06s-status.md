# Packet 06s status, pass 1 (full-screen compare)

Branch `ui/06s-fullscreen`, worktree `/mnt/Sabrent/homelab/.alt/ui-06s`, based on `fc73c64`. Ports 41740-41759. Nothing is pushed or merged. The dev server on 41740 was stopped.
Read first: `plans/packets/06s-fullscreen-compare.md`, then the approved prototype `/mnt/Sabrent/homelab/.alt/compare-marks-design/combo.html` (open with `#e` and `#e,dark`).
Rules for the next worker: `node_modules` is a shared symlink (never `bun install`). Several files are CRLF (`src/App.svelte`, `FileCompare.svelte`, `FolderCompare.svelte`, `CompareDetails.svelte`, `Icon.svelte`, `state.svelte.ts`, `src-tauri/capabilities/default.json`). Edit them with a CRLF-aware replace, never a text-mode rewrite. Stage files by name; `node_modules` is untracked. No explanatory comments. Do not edit `README.md` or Rust.

## Steps

The numbers follow the brief's scope list, not the packet's own step numbers.

| Brief item | State | Commit |
| --- | --- | --- |
| 1 Full-screen shell | done | `80b65b9` |
| 2 Marking E (bars, glyphs, gap stripe, ribbon gutter, large-file renderer) | done | `1dbe6a2` |
| 3 Overview ruler | done | `2a2637d` |
| 4 Chrome: header, pane row, footer, per-change copy, confirm bar, Save right | done | `367a8fc` |
| 5 Keyboard, accessibility, reduced motion, 1100 px, contrast, final screenshots | partial | `ff50bf8` |

No uncommitted files remain at the time of writing, except this file's own commit.

### 1 Full-screen shell
- `src/lib/fullscreen.svelte.ts`: class `FullScreen` (`enter`, `exit`, `toggle`, `follow`, `back`) and `escapeLeaves`. The window API is injected, so the unit test mocks it.
- `src/lib/compare-fullscreen.ts`: the singleton `compareFullscreen`, wired to `getCurrentWindow()` only when `isTauri()` is true. The browser build only switches the layout.
- `src/App.svelte`: `immersive` derived, a `follow` effect, F11 and Esc in `onKey`, class `immersive` on `#shell`.
- `src/styles/fullscreen.css`: hides rail, tabs, sidebar, details panel and status bar in `#shell.immersive`. Persisted workspace layout is never touched, so restoring is automatic.
- `src-tauri/capabilities/default.json`: added `core:window:allow-set-fullscreen` and `core:window:allow-is-fullscreen` only.
- `src/lib/test-support/app-lifecycle.js`: extra bindings so the App script still evaluates in tests.
- Behaviour: opening a compare or file-diff tab enters full screen; Back, Esc and F11 leave it. Back and Esc activate the tab that was active before the compare. F11 pressed on a compare tab belongs to the compare. F11 pressed elsewhere keeps OS full screen when a compare opens and closes. Esc does nothing while a dialog, `.compare-menu`, `.cm-search`, `.confirm-bar`, an expanded control inside the active compare tab, or a text field is involved.
- The restore-maximized fallback calls `maximize()` when the window was maximized and the OS did not restore it. That needs `core:window:allow-maximize`, which I did not add (packet says add only two permissions). Decide: add it, or drop the fallback.

### 2 Marking E
- `src/lib/editors/merge-marks.ts`: `changeMarks` and `changeMarkGutter` extensions, `markCurrentChange`. A ViewPlugin builds line decorations (`cm-mk`, `cm-mk-add|rem|chg`, `-first`, `-last`) and the glyph gutter from `getChunks(state)` for the viewport. A `requestMeasure` pass writes `data-mk` on `.cm-mergeSpacer` and `.cm-deletedChunk` so the gap stripe takes the change tone. The current change tints all gutters through `gutterLineClass`.
- `src/lib/editors/mark-lines.ts`, `chunk-bands.ts` (pure, tested): marked lines, bands, ruler marks, thumb maths.
- `src/lib/editors/ribbon-path.ts` (pure, tested) and `ribbon-gutter.ts` (DOM, SVG, windowed to the viewport plus 4 chunks, click jumps).
- `src/lib/editors/side-by-side-surface.ts`: mounts the 56 px gutter between the editors, `bands()`, `onLayout()`, `markCurrent()`. `inline-surface.ts` has the same members with approximate bands. `surface.ts` has the `Surface` additions and `LayoutEvents`.
- `src/lib/editors/merge-theme.ts`: tone bar, glyph column, hatched `.cm-mergeSpacer`, changed-character fill. `src/styles/editor.css`: ribbons, ruler, scrollbar hiding, inline height fix.
- `src/lib/editors/viewer-editor.ts` plus `editor.css`: large-file renderer has the bar, glyph and gap stripe (classes `is-added`, `is-removed`, `is-changed`, `is-void is-gap-ok|err`). It has no ribbons and no ruler.
- Interface `src/lib/editor.ts` is unchanged. `MergeEditor` gained a private `jump(index)` used by ribbon and ruler clicks, and marks `max(0, current)` as current.

### 3 Overview ruler
- `src/lib/editors/overview-ruler.ts` (`OverviewRuler`) and `surface-ruler.ts` (`withRuler`, called from `MergeEditor.mount`). 22 px strip, three lanes (removed, changed, added), visible-window thumb, click on a mark jumps, click on the strip or drag scrolls, recomputed on layout events.

### 4 Chrome
- New components in `src/components/file-compare/`: `Header.svelte`, `PaneHeads.svelte`, `Footer.svelte` (rewritten), `ConfirmBar.svelte`, `MoreMenu.svelte`. Old `Toolbar.svelte` and `Endpoints.svelte` are deleted.
- `src/components/FileCompare.svelte` composes them. Per-change copy arrows open the confirm bar; confirming calls the existing `copy(side)` path. Ctrl+Alt+Left/Right still copy at once. Save right runs the existing `editor-save-right` command. Whole-file copies, undo and "use language for all .ext files" are in the more menu (they use the existing commands).
- `src/lib/change-summary.ts` (pure, tested): `countChanges`, `describeCopy`.
- `src/styles/file-compare.css`: new chapter; dead `.editor-toolbar`, `.editor-endpoints`, `.editor-footer` rules removed from `comparison.css` and `control-overrides.css`. Class `editor-endpoints` stays on the pane row as a test hook.
- New icons in `Icon.svelte`: `save`, `swap`, `arrowLeft`, `arrowRight`.

### 5 Keyboard (partial)
Done in `ff50bf8`:
- `src/lib/commands.ts`: `shortcut(event, typing)` returns N and P for next and previous change unless an editable `.cm-content` has focus; palette command `fullscreen` (F11). `App.svelte` passes `typing`.
- `FileCompare.next()`: the first N moves from the shown first change to the second (the editor contract still starts at -1; this is handled in the component).
- `merge-extensions.ts`: removed `indentWithTab`, so Tab leaves the editors (no keyboard trap). Ctrl+] and Ctrl+[ still indent. Open question for the owner.
- Reduced motion: the confirm bar animation is off under `prefers-reduced-motion` (checked in the harness). Other new UI has no animation.
- Contrast: all pairs pass (table below).
- Screenshots exist (see below) but I only looked at some of them.

## Commands and last results

Run in `/mnt/Sabrent/homelab/.alt/ui-06s`:
- `bun run --bun check`: 0 errors, 0 warnings (last run after the final edit).
- `bun test src/lib`: 643 pass, 0 fail.
- `bun run --bun build`: built.
- `bun scripts/testing/css-order.ts`: manifest matches. Use `--write` after any CSS change.

Harnesses (start the server first: `bun ./node_modules/vite/bin/vite.js --port 41740 --strictPort --host 127.0.0.1`, stop it by PID afterwards). Always set `PLAYWRIGHT_DIR=/home/claud/spikes/editor-spike/node_modules/playwright-core`:
- `node scripts/testing/browser/ui-06s.mjs http://127.0.0.1:41740/ <shots> <light|dark> <1440|1100>`: 44 pass, 0 fail in all four combinations (last run).
- `ui-37.mjs <url> <shots> light 1440`: 31 pass, 0 fail. `ui-37.mjs ... dark 1100` passed 13 before the inline height fix; rerun it.
- `ui-46.mjs <url> <shots> light 1440`: 26 pass, 0 fail (before the last small edits; rerun).
- `ui-44.mjs <url> <shots> light 1440` and `dark 1100`: 122 pass, 0 fail (run after step 4; rerun after the next change).
- `ui-37` and `ui-46` were already broken at the base commit (they used the pre-packet-44 rail). I fixed their navigation through `scripts/testing/browser/open-compare.mjs`. They also needed: `__saved` renamed `__fileSaves` (the mock overwrote it), pane width bound, F11 before "Apply rules" (the details panel is hidden in full screen), `is-changed` for the large file, new copy-arrow flow, F7 semantics.
- ui-06s uses `window.isTauri = true` plus recorded `plugin:window|set_fullscreen` calls to prove the window API calls.

Screenshots from my last run: `/tmp/claude-1000/-mnt-Sabrent-homelab-paperwing/6b413f7d-479e-4041-bd20-aeacd59ef039/scratchpad/06s/h06-<theme>-<width>/06s-<theme>-<width>-<name>.png` with names `folder`, `file-first`, `file-second`, `confirm`, `regular`, `many`, `large`. Prototype references: `.../scratchpad/06s/proto-e-light.png` and `proto-e-dark.png`. The scratchpad may be cleaned; rerun ui-06s to regenerate.

Contrast (tokens are the same as the prototype; both themes). Script: `.../scratchpad/06s/contrast-table.mjs` (uses `~/.agents/rules/tools/contrast.mjs` and `oklch.mjs`). Lowest results: light ok bar 3.45:1, light ok ruler mark 3.09:1, light ruler window outline 3.00:1 (borderline), light err/warn/ok text tone on editor 5.7 to 6.95:1, light editor text on 30% tone fill 9.08 to 11.55:1, dark all above 5.1:1. Every graphic pair is at least 3:1 and every text pair at least 4.5:1. The hatch stripes (1.4 to 1.9:1) are decorative; the dashed bar and glyph carry the meaning.

## Known bugs and open questions
- CodeMirror's diff (`scanLimit` 500 in `merge-diff.ts` default) collapses files over roughly 800 lines with scattered changes into one chunk. This is the base behaviour; the ruler and ribbons then show one change. Not changed.
- Focus ring on the language `Select` trigger is only a border colour change (existing component style). Not changed.
- Esc inside an editable editor leaves full screen immediately. The CodeMirror "Esc then Tab" escape no longer applies because Tab is not trapped any more.
- The packet's "Windows: title bar, window controls and drag region still work in the full-screen view" conflicts with the owner's OS full screen decision (no title bar). I hid the tab strip with the window controls in full screen. Ask the owner.
- Inline layout: the pane row still shows two heads and the middle copy arrows over a single column. The inline gutter has glyphs and bars but no ribbons. Inline band heights for pure removals are approximate (`defaultLineHeight` times lines).
- The large-file renderer has no ribbons, no ruler and no per-change copy (it is read-only).
- The viewer engine's pane row aligns to a 56 px middle column that the viewer does not have (about 28 px offset).
- No Windows run has been done (packet step 6). Manual check needed: real `setFullscreen`, restore of a maximized window.
- I did not run the reviewer. Not done: line-details panel changes (left as is, hidden in full screen).

## Remaining in this pass
1. Step 5 finish: look at every screenshot at 1440 and 1100 in both themes and compare with the prototype (I viewed light and dark confirm at 1100, light 1100 file-first, light 1440 confirm, regular, inline). Check folder and large images too.
2. Keyboard-only run beyond the Tab order already tested (ruler marks are `tabindex=-1` by design; N and P serve them). Add a short keyboard map to docs only if the owner wants it (README is off limits in this pass).
3. Decide the open questions above (maximize permission, Tab-indent, Esc in editor).
4. Rerun all harnesses and gates once after the last change, then report with the contrast table and screenshot paths.

## Pass 2 (not started)
- Step 2 of the packet: folder twin trees (aligned rows, filters all, differences, same, orphans, status markers). `FolderCompare.svelte` currently only has the Back button and the existing table; at 1100 px the table drops the Modified columns through an existing media rule.
- Step 4 of the packet: Commits tab with the two-rail graph (`HistoryGraph.svelte`, `src/lib/history-graph.ts`) for the two endpoints, tested on fixture repositories with diverged branches.
- The folder compare hides the details panel in full screen, so "Comparison rules" and "Apply rules" are only reachable after F11. Pass 2 should give them a place.
