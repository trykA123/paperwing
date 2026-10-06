# 37 — Replace Beyond Compare

Status: ready (owner chose the faster editor, 2026-10-06). Delivered together with 06s.
Platform: Windows first, Linux parity
Size: L
Role: both (editor and engine work first, then integrations)

## Goal
Skein does what the owner uses Beyond Compare for, inside Git workflows and on plain folders: text and folder compare, editing and safe saving on both sides, three-way merge, and `git difftool` and `git mergetool` integration.

## Already done
- Compare engine and views: `src-tauri/src/compare.rs` and `compare/`, `src/components/FolderCompare.svelte`, `FileCompare.svelte` with `file-compare/` (Endpoints, Toolbar, Footer), `SetCompare.svelte`, `DiffViewer.svelte`, `CompareDetails.svelte`, `CopyOperations.svelte`.
- Editing and saving go through the platform write paths: `src-tauri/src/files.rs` (Windows) and `src-tauri/src/linux_files/`, with recovery records. Editor commands (save left or right, hunk copy, undo) are in `src/lib/commands.ts`.
- Editor today: `monaco-editor` 0.57.0 (`package.json`, `src/lib/monaco.ts`, `src/lib/editor.ts`).
- Launch hooks: `compareFolders` and `openFolder` launch actions (`LaunchAction` in `src/lib/api.ts`, `src-tauri/src/launch.rs`).
- Editor spike (done): `/home/claud/spikes/editor-spike/results.json` and `results/raw.jsonl` (3 runs per cell, medians, 1440x640 editor, WebKitGTK as the Tauri Linux engine and Helium as the WebView2 proxy). Candidates: Monaco diff editor (also trimmed, inline and plain-text variants), CodeMirror 6 merge view (also line-level and unified variants), and a custom renderer.
- Missing: three-way merge, sessions, command line and git tool integration, folder sync, binary and image compare, text rules beyond the current set.

## Decisions
- Editor choice (owner, 2026-10-06: "take the faster one"): CodeMirror 6 merge view replaces Monaco. Files above 5 MB, or where CodeMirror scrolls below 30 fps, open in the custom renderer read-only with a notice. Build both behind one adapter interface (`src/lib/editor.ts`) so the choice changes one module. Remove `monaco-editor` once nothing imports it.
- Spike facts for the review (medians, Helium and WebKitGTK):
  - 1 MB file (31k lines): Monaco interactive 274 ms and 494 ms with 508 MB and 923 MB extra memory; CodeMirror merge 101 ms and 155 ms with 101 MB and 66 MB; custom renderer 50 ms and 47 ms.
  - 5 MB: Monaco 382 and 591 ms, 847 and 1271 MB; CodeMirror 214 and 259 ms, scroll 19 and 22 fps; CodeMirror line-level variant scroll 47 and 51 fps.
  - 20 MB: Monaco 1107 and 1579 ms, 1569 and 2400 MB, 3 errors on Helium; CodeMirror scroll 7 fps; custom 357 and 371 ms at 60 fps.
  - Single-line 2 MB JSON: Monaco 5.0 s and 5.6 s; CodeMirror 35 and 52 ms.
  - Bundle, gzip: Monaco about 1.05 MB trimmed (3.4 MB full); CodeMirror about 281 KB; custom about 3 KB.
  - Open question for the review: the custom renderer is fast but its editing, search and highlighting scope was not measured as full features. `results.json` has no round-trip (BOM and EOL) data.
- Large-file mode (virtualised read-only above an editor limit) is required whatever editor wins.
- Every save uses the platform write path (tickets, fresh authority, recovery records). No generic file write.
- BOM and line endings are preserved byte for byte on save.
- Git tools: `skein diff <a> <b>` and `skein merge <base> <local> <remote> <output>` plus a documented config snippet. Skein never writes global Git config.

## Scope
- Do: editor adapter; side-by-side and inline; text rules; folder compare filters and sync preview; three-way merge; binary, image and large-file modes; sessions; command line; difftool and mergetool.
- Do not: choose the editor before the review; add a generic file write path; build the full-screen layout (06s owns it).

## Read first
- `/home/claud/spikes/editor-spike/results.json` and `results/raw.jsonl`
- `plans/packets/06s-fullscreen-compare.md`
- `src/lib/editor.ts`, `src/lib/monaco.ts`, `src/components/FileCompare.svelte`, `src/components/FolderCompare.svelte`
- `src-tauri/src/compare.rs`, `src-tauri/src/files.rs`, `src-tauri/src/linux_files/`, `src-tauri/src/launch.rs`
- `docs/linux-write-contract.md`

## Do not touch
- The write paths themselves (06 and the Linux file service own them); call them.

## Steps
1. Define the adapter interface, port the compare view onto CodeMirror 6 merge view, and add the custom renderer for large files. Check: existing compare tests pass; the adapter has a contract test that any editor must pass (save fidelity, BOM and CRLF round trip, change navigation).
2. Text rules: ignore whitespace (leading, trailing, all), case, line endings, regex lines; encoding detection and explicit choice; binary or text override. Check: Rust and unit tests on fixtures.
3. Folder compare: filters (all, differences, same, orphans), name filters and exclude globs, compare by size and time, content, or Git blob id; copy with confirm; sync preview (mirror, update). Check: fixture trees; Windows VM run with long paths.
4. Three-way merge: base, left, right, output; accept left, right or both per chunk; mark resolved; save through the platform path. Check: fixture conflicts from real Git merges.
5. Binary and hex compare, image compare (side by side, swipe, difference), large-file mode. Check: fixtures above the editor limit.
6. Sessions: save and reopen a comparison (paths, refs, rules); recents in the Compare rail section. Check: unit tests; stored in the 34 store when merged.
7. `skein diff` and `skein merge` command line and the difftool and mergetool snippet; "Compare with Skein" from the file manager (packet 25). Check: a real `git difftool` and `git mergetool` run on Linux and on the Windows VM.

## Done when
- The owner's Beyond Compare workflows (text edit and save, folder sync, Git merge conflicts, binary and image checks) run in Skein on Windows and Linux.
- `git difftool` and `git mergetool` open Skein.
- Saved files keep their BOM and line endings.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- The owner's editor decision is still missing when step 2 is done.
- A save path cannot keep BOM or line endings.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.
