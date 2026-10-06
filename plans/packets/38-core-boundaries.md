# 38 — Core boundaries: skein-core, capability registry, event bus

Status: ready (34 merged 2026-10-06). Must land before 31 (Actions) and 35 (Jira).
Platform: both; no platform-specific behaviour
Size: L
Role: api-builder (gpt-6.1-sol xhigh), one writer; frontend only for the enable/disable setting

## Goal
Turn the Rust backend into a small core with stable contracts, so new integrations (Jira, Actions, Jenkins, internal company systems) plug in as providers without touching the core or the UI. A disabled integration costs nothing: no worker, no polling, no API traffic, no cache.

Source: owner's architecture notes "Încăpățânatul × Arhitecta" (2026-10-06). Mantra: core small, contracts stable, capabilities optional, providers replaceable, workers disposable, state recoverable, cache bounded, foreground first.

## Already done
- One crate `src-tauri` (`skein_lib`). About 104 `#[tauri::command]` functions registered in one flat list in `src-tauri/src/lib.rs`.
- Modules call each other directly. Events go out through ad hoc `app.emit(...)` calls in `discover_job.rs`, `search_service.rs`, `clone.rs`, `launch.rs`, `credentials.rs` and `git/runner.rs`.
- GitHub lives in `src-tauri/src/github.rs` and `github/` (`http.rs`, `listing.rs`, `cache.rs`, `pulls/`). Sources (github.com, GitHub Enterprise hosts, manual) are configured in settings.
- Packet 34 adds a Rust-owned SQLite store (`src-tauri/src/store/`).

## Decisions
- **No daemon now.** The core is a library crate hosted in-process by the Tauri app. A separate `skein-daemon` host can come later without changing the core API, only if measurements show cold start still hurts.
- **Capabilities are typed traits**, not one generic interface: `RepositoryProvider`, `PullRequestProvider`, `CiProvider`, `IssueProvider`. A provider implements the traits it supports.
- **The registry owns lifecycle.** A provider is constructed only when enabled. The enabled flag lives in SQLite (store from 34). Disabling drops it: its tasks stop and its cache rows are removed.
- **Event bus is in-process first**: a `tokio::sync::broadcast` of one serializable `CoreEvent` enum (`RepoOpened`, `RepoCloned`, `BranchChanged`, `PullRequestUpdated`, `CiStarted`, `CiCompleted`, `IssueUpdated`, `ProviderHealthChanged`). The Tauri layer forwards events to the UI. The enum must be serde-serializable, so moving a provider out of process later needs no redesign.
- **Out-of-process workers only when a real integration needs one.** Not in this packet.
- Keep the keyring service name `paperwing` and the app identifier `dev.paperwing.app` (user data compatibility).

## Scope
- Do: workspace with `skein-core`; thin Tauri command layer; registry; event bus; GitHub moved behind provider traits; an enable/disable setting per provider.
- Do not: new integrations, a daemon, a plugin ABI, network services, behaviour changes. Every existing command keeps its name, arguments and results.

## Read first
- `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`
- `src-tauri/src/github.rs`, `src-tauri/src/github/` (all)
- `src-tauri/src/store/` (after 34)
- `src-tauri/src/discover_job.rs`, `src-tauri/src/search_service.rs` (the existing job and event pattern)
- `src/lib/api.ts` (one place for IPC types)
- `~/.agents/rules/rust.md`, `~/.agents/rules/code-quality.md`

## Steps
1. Create a Cargo workspace: `src-tauri/core` (crate `skein-core`) and the existing app crate. Move pure modules first (git runner and validation, compare, history, tags, stash, branch cleanup, search, store), with their tests. The app crate keeps only `#[tauri::command]` wrappers and setup. Check: `cargo test --offline --workspace` passes with the same test count as before; `bun run --bun build` and a Linux `tauri build --no-bundle` succeed.
2. Split command registration by domain: each domain module exposes its handler list, and `lib.rs` composes them. Check: the command list diff shows the same 104 names.
3. Add `CoreEvent` and the bus in `skein-core`. Replace the ad hoc `app.emit` calls with publishing on the bus, and add one forwarder in the app crate that keeps the existing frontend event names. Check: the frontend tests pass unchanged, and a new test proves each previous event name still arrives.
4. Add the capability traits and the registry. Move GitHub listing, refs and pulls behind `RepositoryProvider` and `PullRequestProvider`, keyed by source (host) so several Enterprise hosts plus github.com each get their own provider instance. Check: existing GitHub tests pass. New tests: a disabled provider is never constructed, makes no HTTP calls (counting transport) and leaves no cache rows.
5. Persist enabled providers in SQLite. Add a Settings toggle per source. Check: toggle off → restart → still off and no traffic; toggle on → listing returns.
6. Write `docs/architecture.md`: the layers, the traits, the events, and the 10-question checklist from `plans/packets/TEMPLATE.md` that every new integration must answer.

## Done when
- The workspace builds and tests pass on Linux and on Windows CI.
- No command name, argument or result changed (api.ts diff is empty apart from new provider settings).
- A disabled provider shows zero HTTP requests and zero cache rows in a test.
- 31 and 35 can be written against `CiProvider` / `IssueProvider` without editing the core.

## Stop and report if
- Moving a module needs a behaviour change.
- Tauri types leak into `skein-core` and cannot be removed simply.
- Compile time or binary size grows by more than 20 percent.

## Report
Shas per step, the before/after test count, the command-name diff, the event-name compatibility test, and the gate results.
