# 36 — Frontend performance

Status: blocked by 20 (progressive UI) and 34 (store queries); step 1 can start now
Platform: Windows first (WebView2, Defender on), Linux (WebKitGTK) parity
Size: L
Role: ui-builder-high, one writer

## Goal
Startup, scrolling, filtering and opening files stay smooth at 10,000 rows and multi-megabyte files, proven by measurements on Windows and Linux.

## Already done
- Benchmark hooks: `src/lib/benchmark.ts` records `ui.request`, `ui.files-ready`, `ui.first-render`, `ui.complete`, `editor.import`, `editor.construct` and `editor.diff` when `VITE_PAPERWING_BENCHMARK=1`, through the Rust `benchmark_record` command (`src-tauri/src/benchmark.rs`).
- `src/components/VirtualList.svelte` exists and is used by the compare lists.
- Monaco is imported lazily in `src/components/FileCompare.svelte` (`await import('../lib/monaco')`), but `src/lib/monaco.ts` bundles every worker (editor, css, html, json, typescript) and the editor is `$state.raw` there only.
- Fonts: `@fontsource-variable/geist` and `geist-mono` are installed (full set, not subset).
- Packet 17 (landing) adds binary content IPC (`src/lib/content-bytes.ts`).
- Not present: Tauri `Channel` streams, per-frame batching, code-split views, a 5 MB large-file path, a scroll fps harness.

## Decisions
- Measure first. Targets below are proposals; the owner ratifies them after step 1.
- Large results use `$state.raw` with whole replacement. Rows keyed by stable id. Counts and totals come from Rust.
- Progressive batches apply at most once per animation frame.
- Streams use a Tauri `Channel`. File bytes use binary `Response`, never `number[]`.
- One long-lived diff editor with model swapping and disposal. Minimap, bracket colouring and whitespace rendering are off for large files. Load only the needed languages on demand. Above about 5 MB, show a virtualised Rust-computed text diff instead of Monaco.
- The editor choice (Monaco or CodeMirror 6 merge view) comes from the editor spike. If the spike picks CodeMirror, apply the same rules to it and drop the Monaco items.
- No `backdrop-filter` or large blurs on WebKitGTK; measure renderer settings.
- Code-split Settings, compare, Actions and Jira views. Geist Latin subset only. First paint comes from the store (34).
- No behaviour change. Every existing test passes.
- Measurement method: release builds, process-cold versus app-warm versus cache-hit kept apart, five warm-ups plus twenty samples (ten for expensive cold runs), p50 and p95, hardware and tool versions recorded, UI tasks over 50 ms counted, network time separated from local work.

## Scope
- Do: marks, the harness, and the changes below.
- Do not: visual redesign, backend caching (21), changing comparison semantics.

## Read first
- `src/lib/benchmark.ts`, `src/lib/compare-state.svelte.ts`, `src/lib/compare-view.ts`, `src/lib/monaco.ts`
- `src/components/FileCompare.svelte`, `FolderCompare.svelte`, `VirtualList.svelte`, `HistoryDrawer.svelte`
- `src/main.ts`, `src/App.svelte`, `src/lib/fonts.ts`
- `plans/packets/20-progressive-frontend.md`, `docs/testing.md`
- `~/.agents/rules/typescript.md`, `~/.agents/rules/web-ui.md`

## Do not touch
- Rust compare code (17, 19, 21) and the progressive state machine owned by 20 (build on it after it lands).

## Steps
1. Measure: marks for first row, full result, file open and filter latency; frame times while scrolling 10,000 rows on `skein-fixture-mono` and a 10,000-row synthetic list; run on Windows (VM) and Linux; record the before table. Check: a repeatable `bun scripts/testing/` runner, or the documented manual steps, and the table in the commit message.
2. Reactivity and batching: `$state.raw`, stable keys, once-per-frame batch application, Rust-side counts. Check: `bun test src/lib`; the list-render and filter timings improve.
3. IPC: `Channel` for streams, binary `Response` for bytes. Check: no `number[]` payloads remain (grep and a test).
4. Editor: on-demand languages and workers, one long-lived diff editor with disposal, large-file settings, the 5 MB path. Check: a 5 MB pair opens within target; memory returns after close; no leaked models.
5. Virtualise history, logs, ticket and compare trees; add `contain: content` on rows. Check: 10,000-row scroll stays at 60 fps.
6. Startup: code-split views, Latin font subset, first paint from the store. Check: bundle sizes before and after; startup time on Windows.
7. WebKitGTK: remove blur and backdrop filters, test renderer settings. Check: scroll fps on Linux.

## Done when
Proposed targets, ratified after step 1:
- First rows within 150 ms on Linux and 300 ms on Windows for the fixture comparison.
- 60 fps scrolling at 10,000 rows on both.
- A 5 MB file opens in under 500 ms.
- The before and after tables exist for both platforms. No existing test regressed.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (`--write` if styles changed)
- Screenshots at 1440 and 1100 px, both themes; Windows VM run

## Stop and report if
- Targets are unreachable without a behaviour change.
- The editor decision from the spike is still open.

## Report
Commit sha, before and after tables per platform, bundle sizes, screenshots, gate results.
