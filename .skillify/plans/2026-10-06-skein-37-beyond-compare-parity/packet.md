# Packet 37 — Replace Beyond Compare

| | |
|---|---|
| Status | Approved 2026-10-06 (owner: "I want to replace Beyond Compare", editing and saving required). |
| Weight | Heavy (new compare engine and editor). |
| Depends on | 14 (writes), 17 (batched diff), editor spike result (packet 36 R4b); delivered with Skein step 6. |

## Goal
Skein does everything the owner uses Beyond Compare for, inside Git workflows and on plain folders, with editing and safe saving on both sides.

## Requirements
- **R1 — Editor.** Chosen by the measured spike (CodeMirror 6 merge view vs Monaco vs custom): side-by-side and inline, both sides editable when writable, syntax highlighting, search and replace, go to line, per-change copy left/right, change navigation (N/P), overview strip, line-details panel with character-level differences, collapsed unchanged regions with context control, Save left / Save right / Save all.
- **R2 — Text rules.** Ignore whitespace (leading, trailing, all), ignore case, ignore line endings, ignore lines matching regex, treat as binary/text override, encoding detection and explicit encoding choice, exact preservation of BOM and EOL on save.
- **R3 — Folder compare.** Aligned twin trees, filters (all, differences, same, orphans), name filters and exclude globs, compare by size/time, by content, or by Git blob ID, expand/collapse, copy left/right with confirm, sync preview (mirror, update), per-row status.
- **R4 — Three-way merge.** Base, left, right and output panes for Git conflicts; accept left/right/both per chunk; mark resolved; writes through the packet 14 save path.
- **R5 — Other formats.** Binary/hex compare, image compare (side by side, swipe, difference overlay), and large-file mode (virtualised read-only above the editor limit).
- **R6 — Sessions.** Save and reopen a comparison (paths, refs, rules); recent sessions in the Compare rail section.
- **R7 — Integrations.** `skein diff <a> <b>` and `skein merge <base> <local> <remote> <output>` command line; register as `git difftool` and `git mergetool` (documented config snippet, never auto-written to global config); "Compare with Skein" from the file manager (packet 25).
- **I1** — Every save uses the platform write path (tickets, fresh authority, recovery records); no generic file write.

## Done when
The owner's own Beyond Compare workflows (text edit and save, folder sync, Git merge conflicts, binary/image checks) run in Skein on Windows and Linux, and `git difftool`/`git mergetool` open Skein.
