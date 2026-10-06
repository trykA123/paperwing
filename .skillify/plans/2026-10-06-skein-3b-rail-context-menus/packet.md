# Skein step 3b — Activity rail sections and context menus

| | |
|---|---|
| Status | Approved 2026-10-06 (owner proposal). The rail shell (Sets, Compare, Recovery, Activity, Settings) ships with Skein step 3. |
| Depends on | Skein step 3; sections Changes (26/29), Pull requests (28) and Actions (31) are added by those packets. |

## Goal
The left side is an activity rail with switchable sidebar panels, and right-click menus reach every common action, including compare.

## Requirements
- **R1 — Compare panel.** "New compare" picks any two of: repository, branch, tag, commit, folder, snapshot; recent and pinned comparisons; set-wide compare.
- **R2 — Context menus.** One accessible ContextMenu component (keyboard: Shift+F10 and the Menu key; arrow navigation; typeahead) used on:
  - repository rows: Compare with… (another repo, origin, branch, tag, snapshot), History, New branch, Tag, Stash, Open in VS Code, Open in terminal, Copy path;
  - two selected rows: Compare these two;
  - branches and tags in the ref picker and history drawer: Compare with current, Check out, Tag here;
  - folder and file compare rows: Copy to left or right, Open, History.
- **R3 — Badges.** Rail icons show counts (PRs to review, failed runs, unresolved recovery records) once their sections exist.
- **I1** — Every menu item maps to an existing command in commands.ts; the menu adds no new mutation path.

## Done when
Every listed menu works by mouse and keyboard in Helium at 1440 and 390 in both themes, and "Compare these two" opens the compare view.
