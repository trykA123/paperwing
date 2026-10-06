# 30 — Automatic refresh

Status: ready for the file watcher; the timed fetch waits for 19 (foreground priority)
Platform: Windows first, Linux parity
Size: M
Role: api-builder, then a small ui-builder step for the settings and notice

## Goal
Local status updates by itself when files change in an open set. An optional timed fetch keeps "behind" counts current without the user pressing Fetch.

## Already done
- Nothing watches the file system. `src-tauri/Cargo.toml` has no `notify` crate.
- Manual refresh exists: command "Refresh local status" in `src/lib/commands.ts` and the status loader in `src/lib/state.svelte.ts`. Cache invalidation and ref epochs exist (packet 16, `src/lib/state/repository-metadata.svelte.ts`, `docs/metadata-caching.md`).
- Git activity and busy state: `src-tauri/src/git/runner.rs`, `src/lib/state/git-activity.svelte.ts`.

## Decisions
- Use the `notify` crate, pinned to an exact version, with its recommended watcher per platform. On Windows this is `ReadDirectoryChangesW`; there is no polling fallback unless the watcher errors.
- Debounce 300 ms per repository. Ignore `.git/objects`, `.git/lfs`, `node_modules`, `target`, `dist`, `build`. Keep `.git/HEAD`, `.git/index` and `.git/refs` events: they signal branch and stage changes.
- Refresh only the affected repositories, and only for visible or selected rows first.
- A watcher error or a set with too many repositories (default limit 200 roots) falls back to manual refresh and shows one notice. Never fail silently.
- Timed fetch is off by default, interval N minutes in settings, lowest priority, never during a foreground Git command, one repository at a time. Pause on battery saver or metered network where the platform reports it; otherwise run and say nothing.
- Watching network shares and OneDrive placeholders is best effort. On Windows, a root under a cloud-sync folder gets a one-line note, not an error.
- Defender: the watcher reads no file contents and adds no scanning of its own.

## Scope
- Do: watcher service, debounce, ignore rules, per-set lifecycle, status refresh hook, timed fetch, settings and notice.
- Do not: new Git commands; refresh while a clone, pull or switch runs on that repository (queue one refresh after it).

## Read first
- `src-tauri/src/lib.rs`, `src-tauri/src/local.rs` (status), `src-tauri/src/git/runner.rs`
- `src/lib/state.svelte.ts`, `src/lib/state/git-activity.svelte.ts`, `docs/metadata-caching.md`
- `plans/packets/38-core-boundaries.md` (events go through the bus once 38 lands; until then use one emit)

## Steps
1. Watcher service in Rust: start and stop per set, debounce, ignore rules, emits `repo-changed` with the repository path. Check: Rust tests with a temp directory for burst coalescing, ignored paths, and stop releases handles (`/proc/self/fd` count on Linux; a handle check on the Windows VM).
2. Frontend subscriber: refresh only affected rows through the existing status path. Check: unit test that two events inside the window cause one refresh.
3. Lifecycle: close a set, close the app, or remove a repository stops its watch. Check: watcher count returns to zero in a test.
4. Failure path: error or too many roots shows one notice and leaves manual refresh working. Check: test with an injected watcher error.
5. Timed fetch (after 19): setting, scheduler, pause rules, busy check. Check: unit test with a fake clock for interval, busy skip and one-at-a-time.

## Done when
- Editing a file in a fixture repository updates its row within a second without a manual refresh.
- The watcher count returns to zero after closing the set.
- Windows: a repository under a junction and one in a OneDrive folder behave as documented; no Defender prompts or high CPU on a 50-repository set.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `cd src-tauri && cargo test --offline` (set `PAPERWING_TEST_TMP` to an ext4 directory), `rustfmt --check`, clippy on touched files
- UI: screenshots of the settings row and the notice, both themes

## Stop and report if
- Event volume on a 100+ repository set exceeds a few events per second at idle.
- `notify` needs a feature or version that is not available offline.

## Report
Commit sha, files changed, each step's check result, idle CPU and event counts, gate results, anything skipped.
