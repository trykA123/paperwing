# Changelog

## v0.2.0 - PaperWing preview (2026-10-02)

### Highlights

- PaperWing application, executable, installer, repository name, and folded-paper icon.
- Tabbed workspace, command palette, repository trees, and Git Activity with bounded output.
- Folder comparison between local working trees and read-only Git references without checkout.
- Monaco file comparison, editable working-tree panes, difference navigation, and directional copying.
- Confirmed file/folder copy previews, per-file results, and persisted filesystem recovery.
- Whole-set comparison, including unchecked rows and duplicate checkouts.
- New branches, separate-folder branch checkouts, staged/unstaged diff previews, commit messages, and separate Push actions.
- Local branch deletion and confirmed removal workflows.
- Light/dark/system appearance, bundled fonts, resizable panels, and keyboard shortcuts.
- Existing settings and credential migration, plus an expanded workflow guide.

### Preview limitations

- Final desktop acceptance remains in progress. Test new editing and recovery workflows on disposable repositories.
- Recoverable writes require local Windows NTFS with Transactional NTFS available; unsupported writes fail closed.
- Editor/copy content is limited to 2 MiB per file. Copy batches and multi-repository Git actions are not all-or-nothing transactions.
- The commit list displays at most 2,000 changed files; Git commits the entire staged index.
- Unsigned Windows binaries can trigger SmartScreen warnings.

### Downloads

- `PaperWing_0.2.0_x64-setup.exe`: Windows x64 installer.
- `PaperWing_0.2.0_x64_portable.exe`: Windows x64 executable; requires Git and WebView2.
- `SHA256SUMS.txt`: SHA256 checksums for both binaries.

## v0.1.0 — first flight 🪿

The first release of Flock: clone a whole set of repositories, each at the branch, tag or commit you want, in one click.

### Highlights

- **Sets** of repositories that remember each repo's ref and folder name.
- **Sources**: GitHub Enterprise and GitHub.com via personal access token, plus **Manual URLs** for any other host.
  Tokens live in Windows Credential Manager.
- **Browse, search and ★ favorite** repos of your organizations; virtualized lists with 10/25/50/All paging.
- **Ref picker** for branches, tags and commits: grouped branch lists, full lists without caps, keyboard navigation,
  a **commit graph** with branch/tag labels, and *Ref for selected* with per-ref coverage (`3/4`).
- **Same repo, many folders**: duplicate a row and rename its folder (e.g. `demo-project` -> `application-main`,
  `application-feature`); clashing folders are flagged.
- **Destination layouts**: Flat, or Custom path templates (`{set}\{org}\{folder}`, `{folder}_{ref}`, …) with a live tree.
- **Parallel clones** with progress, optional shallow clones, and safe handling of existing folders
  (fetch & checkout, skip, or re-clone keeping a `.bak` copy). A folder holding a different repository is refused.
- **Local column** showing what each folder is on (↑ ahead, ↓ behind, ● changes) with one-click **Switch**, **Pull**
  (fast-forward only) and **Fetch**.
- **Open in VS Code** for the destination root or any cloned repo.
- **Appearance**: light, dark or system theme; bundled fonts (JetBrains Mono, Inter, Fira Code, IBM Plex, Source Code Pro).
- Resizable table columns and right panel; friendly git error messages with hints.

### Downloads

- `Flock_0.1.0_x64-setup.exe` — per-user installer (no admin rights).
- `Flock_0.1.0_x64_portable.exe` — single executable, no installation.

Both are unsigned for now; Windows SmartScreen may ask you to confirm (**More info → Run anyway**).
