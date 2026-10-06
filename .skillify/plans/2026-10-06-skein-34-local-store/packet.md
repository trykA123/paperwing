# Packet 34 — Rust-owned local store and backend state

| | |
|---|---|
| Status | Approved 2026-10-06 (owner: "strengthen the backend and db with rust"). |
| Weight | Heavy (new persistence layer, moves state ownership). |
| Depends on | 16 (scoped keys), 18 (progressive contract). Must land before 28, 31 and 35. |

## Goal
One embedded database owned by Rust holds caches and indexes, Rust owns background state, and the frontend renders typed, paged queries and change events.

## Decisions
- **D1** — SQLite through `rusqlite` with the bundled build (pinned), WAL mode, one file under the app data directory, versioned migrations embedded in the binary; a failed migration renames the database aside and starts empty (it only holds caches).
- **D2** — `settings.json` stays the user configuration. Tokens stay in the OS keyring. Write authority (tickets, roots, sessions) is never stored.
- **D3** — FTS5 indexes ticket summaries, commit subjects and repository names for search.
- **D4** — A single writer task owns the connection; readers use a small pool; no SQL in the frontend.

## Requirements
- **R1 — Tables:** repositories (registry projection), refs (with ref epoch), commits (graph per repo), pull requests and checks, Actions runs, Jira issues and boards (read-only snapshots), snapshots (packet 27), recent and pinned comparisons, packet 21 immutable facts (optional persistence).
- **R2 — Freshness:** every row carries fetched-at, source scope and version; reads return `stale` flags; stale-while-revalidate replaces the JSON listing cache from packet 16.
- **R3 — Backend services:** repository status refresh, GitHub and Jira clients with shared rate limiting, and the packet 19 scheduler with foreground priority live in Rust and publish change events.
- **R4 — Frontend:** typed paged queries and events replace ad-hoc caches in `src/lib/state/*`; `state.svelte.ts` shrinks below 400 lines.
- **R5 — Startup:** the last known state renders from the database before any network or Git call.
- **I1** — No token or token-derived value in the database. Deleting the database file loses nothing but cache.
- **I2** — Size bounded (configurable, default 512 MiB) with LRU pruning; vacuum on idle.

## Steps
1. Store crate module, migrations, writer task, tests (crash during write, corrupt file, migration failure).
2. Move the GitHub listing cache and commit histories into the store; remove the JSON caches.
3. Status refresh and scheduler events; frontend queries.
4. Measure startup and memory before/after on skein-fixture-mono (Linux and the Windows VM).

## Done when
Cold start shows the last state from the store in under 300 ms on the fixture set, the old JSON caches are gone, and all metadata tests pass against the store.
