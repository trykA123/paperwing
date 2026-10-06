# Changelog

## Unreleased

- Fix Linux folder job admission, trash metadata cleanup and clone verification; preserve Windows source formatting.
- Add guarded Linux clone/reclone and desktop Trash with recoverable moves; failed recycling keeps the set until explicit configuration-only removal. Windows behavior is unchanged.

- Add a repository history drawer: open it from the command palette or the details panel to see working tree, local commits, origin commits and the shared base as two rails, with hover and keyboard details. It reads local refs only and never fetches.
- Redesign notifications and inline alerts: five notification types with status-coloured icon and edge, a Rails loop for loading, and Retry and Show log actions on errors. Errors and loading notices stay until dismissed.
- Rename the product to Skein with the Benzol colour system, Geist and Geist Mono fonts, and the Rails icon. The bundle identifier, settings folder and credential service names are unchanged.
- On Windows the installer is now named Skein. If PaperWing 0.2.0 still appears in Installed apps, uninstall it first. Settings and data are kept because the bundle identifier is unchanged.
- Scope metadata by source, credential and ref epochs; share foreground requests and discover authenticated personal private repositories.
- Add durable Linux parent creation with exact file linkage, restart classification and private cleanup; application writes remain disabled.
- Add durable Linux diff reservations with bounded admission, namespace prepayment and safe cleanup; application writes remain disabled.
- Add durable Linux recovery with verified backups, restart reconciliation, conditional undo and owned cleanup; application writes remain disabled.
- Add bounded Linux file guards, private diff files and responsive root probes; recoverable writes remain disabled.
- Keep local Git usable during credential-store outages, preserve captured redaction after token changes, and reject stale credentials before HTTP dispatch.
- Add shared layout and lifecycle characterization with isolated Linux native benchmark fixtures.
- Extract comparison inventory, text diff, history, registration and tests without changing behavior.
- Extract workspace path helpers, state owners and comparison classes behind existing façades.
- Extract file comparison, set-row and settings presentation components with browser parity checks.
- Split CSS into ordered chapters with source and compiled-output parity verification.
- Extract Git validation, redaction, repository trees, remote refs, runner and tests.
- Record the accepted practical Linux write contract and sacrificial race probes.
- Add native roots, path identity and capability reporting with read-only Linux diffs.
- Add persistent Linux Secret Service credentials, distinct wallet failures and scoped metadata invalidation.
- Use fallible OS entropy for Linux Secret Service DH keys and CBC initialization vectors.
- Preserve source drafts after failed credential changes and remove Manual sources without a wallet.
- Fix Tooltip cleanup during rendering and make the Git timeout fixture deterministic.
- Stop Linux Git process groups and retain cleanup ownership after cancellation or dropped callers.
- Join failed Git stream tasks and keep Activity registration through Linux output cleanup.
- Preserve completed Linux Git outcomes and retain Activity and permits through failed setup cleanup.

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
