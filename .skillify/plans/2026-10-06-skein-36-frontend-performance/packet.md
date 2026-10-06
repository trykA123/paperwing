# Packet 36 — Frontend performance

| | |
|---|---|
| Status | Approved 2026-10-06. |
| Weight | Medium. |
| Depends on | 18–20 (progressive contract), 34 (store queries). Measurement harness first. |

## Goal
Startup, scrolling, filtering and opening files stay smooth at 10,000 rows and multi-megabyte files, proven by measurements on Linux (WebKitGTK) and Windows (WebView2).

## Requirements
- **R0 — Measure first.** Performance marks for time to first row, full result, file open, filter latency, and frame times while scrolling 10,000 rows; run on skein-fixture-mono on Linux and the Windows VM; record before/after.
- **R1 — Reactivity.** Large results use `$state.raw` with whole replacement; rows keyed by stable ids; counts and totals come from Rust.
- **R2 — Batching.** Progressive batches apply once per animation frame.
- **R3 — IPC.** Tauri `Channel` for streams; binary `Response` for file bytes (no `number[]`).
- **R4 — Monaco.** Loaded on first file open; only needed languages, loaded on demand; one long-lived diff editor with model swapping and disposal; minimap, bracket colouring and whitespace rendering off for large files; above about 5 MB a virtualised Rust-computed text diff instead of Monaco.
- **R5 — Virtualisation.** Compare trees, history, logs and ticket lists virtualised; `contain: content` on rows.
- **R6 — Startup.** Settings, compare, Actions and Jira views code-split; Geist Latin subset only; first paint from the store.
- **R7 — WebKitGTK.** No `backdrop-filter` or large blurs; measure renderer settings.
- **I1** — No behaviour change; every existing test passes.

## Done when
Measured targets (ratified after R0): first rows under 150 ms on Linux and 300 ms on Windows for the fixture compare, 60 fps scrolling at 10,000 rows on both, and a 5 MB file opens in under 500 ms.
