# Packet 25 — "Open with Skein" from the file manager

| | |
|---|---|
| Status | Approved by the owner 2026-10-06. |
| Weight | Medium (installer registry entries, startup arguments, folder scan). |
| Depends on | 15 (registered roots, clone destinations); menu entries ship with 24. |
| Primary platform | Windows; Linux parity. |

## Goal
Right-click a folder (or the empty space inside it) and choose "Open with Skein". Skein opens a temporary set of every Git repository under that folder, with live status. Selecting two folders offers "Compare with Skein".

## Requirements
- **R1** — A launch argument with a folder path opens a temporary set. A folder that is itself a repository opens that repository directly.
- **R2** — If Skein already runs, the path goes to the running window as a new tab (single instance).
- **R3** — The scan is bounded: depth 4 by default, stops descending at a repository, skips `node_modules`, `target`, `dist`, `.venv` and similar, recognises worktrees and submodules, and streams results with cancel.
- **R4** — Temporary sets are not saved until the user chooses "Save as set"; closing discards them.
- **R5** — Two folder arguments open a folder compare.
- **R6** — Windows: the NSIS installer writes per-user keys under `HKCU\Software\Classes\Directory\shell\Skein` and `Directory\Background\shell\Skein`; uninstall removes them. Linux: a `.desktop` entry with `MimeType=inode/directory` and a Dolphin service menu.
- **I1** — Opening only reads. Writes keep the existing root checks and write consent.
- **I2** — Launch paths are validated like any user-chosen path; symlinks and junctions are not followed outside the chosen folder.

## Decisions
- **D1** — Use `tauri-plugin-single-instance` (official, pinned).
- **D2** — Windows 11 shows the entry under "Show more options". A top-level entry needs a signed MSIX sparse package; out of scope.

## Steps
1. Startup argument parsing and single-instance forwarding. Check: unit tests for argument shapes; native test launching twice.
2. Bounded repository scanner in Rust with streaming events and cancel. Check: fixture tree with nested repos, worktree, submodule, `node_modules`, symlink loop, 10,000 folders.
3. Temporary set UI and "Save as set". Check: browser test; temporary sets never reach settings.json unless saved.
4. Installer and desktop entries (with packet 24). Check: install, right-click, uninstall on the Windows VM; Dolphin and "Open With" on Linux.

## Done when
Right-click → Skein shows every fixture repository with correct status on both platforms, a second launch reuses the window, and uninstall leaves no menu entry.
