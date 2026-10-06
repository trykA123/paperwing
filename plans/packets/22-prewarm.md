# 22 — Prewarm: habitual, background and temporary heat

Status: blocked by 38 (event bus), 34 (SQLite store), 19 (admission classes), 21a (fact cache). Optional.
Platform: Windows first; a benefit must show on Windows with Defender on
Size: M
Role: api-builder (gpt-6.1-sol xhigh), one writer

## Goal
Quietly prepare local facts for the repositories the user is likely to open, so the UI opens warm, without ever slowing the foreground. Leaving this packet out is acceptable.

## Already done
Nothing in code. This packet builds on:
- Packet 38: the `CoreEvent` bus (`RepoOpened`, `RepoCloned`, `BranchChanged`, `PullRequestUpdated`, `ProviderHealthChanged` and others) and the capability registry.
- Packet 34: the SQLite store (`src-tauri/src/store/`, on branch `feat/local-store`, not merged) for saved sets, MRU and usage signals.
- Packet 19: the admission owner in `src-tauri/src/git/runner.rs`. Packet 21: the fact cache and the app budget in `src-tauri/src/budget.rs`.

## Decisions
- Scheduler: reuse the packet 19 admission owner and add a `speculative` class below `enrichment`. Speculation can never delay interactive or enrichment work. Start with two speculative jobs and four CPU slots, measured, not assumed.
- Heat is event-driven, never based on uptime or the calendar. Heat comes from `CoreEvent`s. Usage signals persist in SQLite; warmed facts live only in the RAM cache (21).
- Three banks:
  - Bank A, habitual hot: saved sets, most recently used, pinned or starred repositories, repositories with repeated use. Highest prewarm priority.
  - Bank B, background candidates: repositories likely to be needed soon. Warmed only when the system is idle (no foreground request in flight, no enrichment queued). Never competes with foreground work.
  - Bank C, temporarily hot: the repository just opened or used. Promoted at once on `RepoOpened`, `BranchChanged` or a focus event, without evicting Bank A. Decays back when unused.
- Memory: no budget of its own; at most 25 percent of the cache budget in packet 21. The budget module shrinks it under low memory.
- Prewarm is local and read-only: only proven immutable or metadata operations. No `fetch`, `ls-remote`, missing-ref acquisition, checkout, stage, commit, push or credential access. Never any remote read, API call or provider traffic, so GitHub hosts (github.com or Enterprise) are never touched. Audit every task's call path.
- A foreground request joins or promotes matching speculative work and never waits behind unrelated speculation. Cancelling one speculative consumer never cancels a foreground consumer or invalidates a session or editor.
- Off by default until the owner approves a measured result. A disable switch always exists. A disabled prewarm has no worker, no queue and no signal collection.
- No manual "Warm" button, no crawling of the whole catalogue, no disk blob cache.

## Scope
- Do: the `speculative` class, a heat tracker fed by the event bus, a SQLite table for usage signals, bank selection, post-first-render warming for visible and focused repositories, a Settings switch.
- Do not: remote work, a disk cache, learned scoring beyond the three banks, anything mutating.

## Read first
- `plans/packets/38-core-boundaries.md`, `plans/packets/21-immutable-cache.md`
- `src-tauri/src/git/runner.rs`, `src-tauri/src/store/` (after 34), `src-tauri/src/budget.rs` (after 21)
- `src/lib/state/repository-metadata.svelte.ts`, `src/components/SetView.svelte` (visible range)
- `docs/metadata-caching.md`

## Steps
1. Find a measured workload that local prewarm improves (navigation or file open) and write the exact local-only task allowlist. Check: traces and request counts. Stop if no benefit or any task can reach the network.
2. Add the `speculative` class with foreground promotion, coalescing of visible context and removal of obsolete queued work. Check: concurrency, flood, promotion, no-consumer and cancel fixtures; zero remote commands or API requests (counting transport).
3. Add the heat tracker. It subscribes to `CoreEvent`s, persists signals in SQLite and picks Bank A, B and C. Check: tests for promotion on open, decay, idle-only Bank B, and that disabling stops collection.
4. Integrate post-first-render warming for the active context. Compare enabled and disabled release runs on the Windows VM with Defender on. Check: startup, cold comparison, file open, cancel and memory distributions; foreground under speculation stays within baseline.
5. Enable by default only if the owner approves the numbers.

## Done when
- Admission is bounded; zero speculative network or mutation; promotion and cancel isolation are correct.
- A reproducible Windows benefit with no foreground regression, or the packet closes as omitted or disabled.

## Gates
- `cd src-tauri && cargo test --offline`, `rustfmt --check`, clippy; Windows VM serial `cargo test --locked`
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`

## Stop and report if
- A task can reach the network, work targets the wrong set, any foreground regression appears, or ownership is unbounded.
- Rescuing an unproven benefit would need new heat mechanisms. Close the packet instead.

## Report
Commit sha, allowlist, enabled versus disabled table, gate results.
