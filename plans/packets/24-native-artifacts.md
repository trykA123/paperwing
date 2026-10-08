# 24 — Native artifacts: acceptance and honest docs

Status: partly done (packaging and CI exist; acceptance and docs remain)
Platform: Windows first (NSIS installer is the main artifact), Linux secondary
Size: M
Role: orchestrator plus deployer; the owner observes the Windows walkthrough

## Goal
Launch the fresh Windows and Linux artifacts outside dev mode, prove the key workflows, and document build, storage, credentials and support honestly. No release, tag, version bump or upload happens here.

## Already done
- `src-tauri/tauri.conf.json`: product `Skein`, binary `skein`, version 0.2.0, identifier `dev.skein.app`, NSIS `installMode: currentUser` with `windows/hooks.nsh`.
- `src-tauri/windows/hooks.nsh`: per-user HKCU "Open with Skein" for `Directory`, `Directory\Background` and `Drive`; removed on uninstall.
- `src-tauri/tauri.windows.conf.json`: custom title bar (`decorations: false`).
- `src-tauri/tauri.linux.conf.json`: deb and AppImage, desktop template `linux/skein.desktop.hbs`, KDE service menu `linux/skein-open.desktop`.
- `.github/workflows/build.yml`: Linux and Windows tests, plus artifact jobs `skein-windows-nsis` and `skein-linux` (deb and AppImage).
- Missing: `docs/platform-support.md`; the README (sections "Install", "Build from source", "Where things are stored") is not yet checked against the real artifacts.

## Decisions
- Windows is verified on a native Windows host with Defender on. A cross-compiled artifact is never called natively verified.
- App data, cache, config, WebView, keyring and test roots are isolated for every launch test (`scripts/testing/native-profile.ts`, `src-tauri/tauri.test.conf.json`). An ignored binary directory is not isolation.
- Do not overwrite or touch existing release assets, history, settings, tokens or journals. No version bump, tag, push of release files or publication.
- View-only support is never called parity. Enterprise live testing, non-reference distributions and unsupported filesystems are listed as limits.

## Scope
- Do: launch and walkthrough of the CI artifacts; hash and revision records; the docs.
- Do not: publishing, signing decisions, new distro claims, edits to `docs/improvements.md`, the tour SVG or historical plans.

## Read first
- `src-tauri/tauri.conf.json`, `tauri.windows.conf.json`, `tauri.linux.conf.json`, `src-tauri/windows/hooks.nsh`
- `.github/workflows/build.yml`, `README.md`, `docs/testing.md`
- Evidence from packet 23

## Steps
1. Download the artifacts of one green CI run on the target commit. Record revision, run id, SHA-256 (`Get-FileHash -Algorithm SHA256` or `sha256sum`) and binary type. Check: hashes match the run's files.
2. Windows: install the NSIS build per-user on the VM with Defender on, in a clean profile. Check: install needs no admin rights; the three "Open with Skein" entries work and are removed on uninstall; no Vite URL or dev-server dependency; dialogs, clipboard, opener and Monaco workers work.
3. Windows walkthrough with the packet 23 fixtures: token and settings restart, clone, compare, editor save, copy, undo, recovery, explicit push. Check: owner-observed; recovery retained after restart.
4. Linux: run the AppImage and install the deb in a clean profile. Check: the same walkthrough; the KDE service menu entry appears; storage paths match the docs.
5. Write `docs/platform-support.md` and update README prerequisites, build, storage, credentials, trash and safety. Record per-OS guarantees and limits. Check: replay every documented command and storage path; `git diff --check`.

## Done when
- Fresh artifacts pass the walkthrough and restart on Windows, then Linux.
- Hashes, headers and revision are recorded; release files and version are unchanged.
- The docs are reproducible and say what is not supported.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- CI `build` workflow green on the commit; owner-observed clean-profile walkthrough

## Stop and report if
- Native Windows evidence is missing, a real profile would be touched, a release or version would change, or a packaged workflow regresses.

## Report
Run id, artifact hashes, walkthrough results per OS, the docs diff, anything skipped.
