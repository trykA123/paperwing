# Packet 24 — Fresh native builds and honest support docs

| | |
|---|---|
| Status | Proposed. Needs owner approval. Does **not** authorize a release. |
| Weight | Heavy (deployment and artifact boundary). |
| Depends on | Accepted 23. |
| Primary platform | **Windows** (work use): the NSIS installer is the primary artifact. Linux bundle is secondary. |
| Read first | `package.json`, Tauri config and Cargo manifest, README prerequisites/storage/safety, packet 01 test profile, complete native parity evidence. |

## Goal
Build fresh Windows and Linux executables and platform bundles, launch them outside dev mode, and document build, storage, credentials and support honestly. Existing release files, history and version stay unchanged.

## Why
- Tauri is configured for NSIS only, and the README prerequisites and storage notes are Windows-first.
- Finding GTK/WebKit on the planning host does not prove a packaged Linux build works.

## Scope
- **In:** fresh standalone executables; Windows NSIS installer; one Linux bundle; launch tests; README and `docs/platform-support.md`.
- **Out:** tags, commits of release files, pushes, uploads, installer publication, version bumps, broad distro-support claims.

## Requirements
- **R1** — Both native release executables and the selected bundles launch and complete the key workflow, restart and recovery matrix outside Vite/dev mode.
- **R2** — Each artifact's revision, configuration and hash, the native prerequisites and the supported Linux envelope are recorded. The Windows NSIS installer remains available.
- **R3** — The README explains Linux build, run, storage, credentials, trash and safety, common behaviour and OS-specific guarantees and limits. View-only support is never called parity.
- **I1** — App data, cache, config, WebView, keyring and test roots are isolated. An ignored binary directory alone is not isolation.
- **I2** — No existing release asset, history, settings, token or journal is overwritten. No version bump or publication.

## Decisions
- **D1** — Keep the base Windows NSIS target and add a Linux overlay config for one bundle (AppImage candidate; a distribution-native package only if required). Verify tool and config support before choosing.
- **D2** — Build and verify Windows first, on a native Windows host, with real-time antivirus on. A cross-compiled artifact is never called natively verified.

## Assumptions
- Native Windows and Linux build hosts and bundle runtime prerequisites are available. Without them, "packaged complete" cannot be claimed.

## Steps
1. **Choose and ratify bundle targets** and the exact Tauri CLI flags from the installed `--help` or pinned official docs; keep the Windows target. Record native dependencies, reference distro, filesystem, session and clean-profile storage paths.
   - Where: proposed `src-tauri/tauri.linux.conf.json`, build/profile runner, README.
   - Check: `bun run --bun tauri build --help`; schema/config validation; full source quality gates. No package installation or cross-compilation claim without approval and evidence.
2. **Build fresh artifacts** into new ignored evidence/output directories, never into `release/` history paths. Record revision, diff, flags, toolchain, hashes and binary platform headers.
   - Check: Windows NSIS build on Windows; `bun run --bun tauri build --no-bundle`; the approved `bun run --bun tauri build --config src-tauri/tauri.linux.conf.json` on Linux; hash every copied artifact (`Get-FileHash -Algorithm SHA256` or `sha256sum`) and inspect the binary type.
   - Trap: packaging a development frontend, or calling a cross-compiled artifact natively verified.
3. **Launch the copied artifacts** in verified isolated profiles, Windows first. Repeat token and settings restart, clone and compare, editor save, copy, undo and recovery, and explicit push with manifest-owned fixtures from 23.
   - Check: owner-observed clean-profile walkthrough in the production WebView on both platforms; no Vite URL or dev-server dependency; Monaco workers, dialogs, clipboard and opener work; recovery is retained.
4. **Write honest docs** for support, build, storage, credentials, safety and testing, linking accepted contracts and reference evidence. Record Enterprise, non-reference distro and unsupported filesystem limits.
   - Where: README and proposed `docs/platform-support.md`. Do not edit the tour SVG, `docs/improvements.md` or historical plans.
   - Check: replay the documented commands and storage checks; inspect the full diff and fresh hashes; `git -c core.whitespace=cr-at-eol diff --check`.

## Done when
- **A1 (native, owner-observed):** fresh executables and bundles pass the key workflows and restarts, Windows first, then Linux. Covers R1, I1, I2.
- **A2 (static and native):** real artifact hashes, headers and revision; bundle targets and prerequisites; releases and version unchanged. Covers R2, I2.
- **A3 (static, owner-observed):** reproducible, honest support, build, storage and safety docs. Covers R3, I1, I2.

## Authority and rollback
The owner decides packaging targets, dependencies and any local bundle installation. One writer works in a dedicated worktree; the main session integrates. Verify fixture restore and the isolated profile before launch. Rollback removes only new copied artifacts, test installations and profiles after approved cleanup, keeps forward recovery support and data, and restores owned build and doc changes. No publication is authorized.

Stop on missing native Windows or Linux evidence, access to a real profile, a changed release or version, or a packaged workflow regression.

## Revision log
- 2026-10-02: Proposed artifact and support acceptance without cutting a release.
- 2026-10-05: Windows NSIS marked primary; Windows-first build and launch (D2). Rewritten in plain format.
