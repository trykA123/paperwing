# 18 — Progressive comparison: contract and UX

Status: ready (packet 17 phase 1 is on main). Design only; gates 19 and 20.
Platform: neutral contract; measured on Windows first
Size: S
Role: orchestrator with reviewer (documentation only, no product code)

## Goal
Write down how a comparison shows useful rows before it is finished while still ending with exactly today's final result. Packets 19 (backend) and 20 (UI) implement this document.

## Already done
- The barrier is real. `Prepared` in `src-tauri/src/compare.rs` classifies normalised bytes before anything is published. `Service::refresh` returns only when preparation is complete.
- The UI waits too. `loadAllFiles` in `src/lib/compare-state.svelte.ts` assigns results after every page arrives; `src/components/FolderCompare.svelte` stays in "loading" until then; `src/lib/compare-view.ts` filters on final status.
- Packet 17 phase 1 adds batch readers and a cheaper `prepare`. It does not publish early.

## Decisions
Proposed here; the owner ratifies them in step 3.
- Show a clearly marked pending inventory early. Supported rows can be selected and opened. Classification, counts and history fill in later.
- Same and Differences totals exclude unclassified rows. A separate "pending" group shows its count. Pending work is never hidden.
- Additive endpoints (candidates `comparison_start` and `comparison_progress`). Legacy commands keep waiting for the final result.
- A different object id is not a final difference after normalisation. Pending rows and counts never pose as final and never authorise a write.
- Final semantics, limits, destination-only retention and dirty-editor guards do not change. Unopened files may still need reads for exact classification, so strict laziness is not promised.
- Enrichment runs in the `enrichment` class defined in packet 19. Memory limits come from the app budget in packet 21.

## Scope
- Do: `docs/progressive-comparison-contract.md` with data types, states, messages, stable row ids per generation, provisional versus final status, unavailable and binary classes, progress, completion and error states, directory and set summaries, filters, selection, rollout and the first-useful-render metric.
- Do not: implement anything, add a cache, redesign visuals.

## Read first
- `src-tauri/src/compare.rs`, `src-tauri/src/compare/{inventory,text_diff,history,registration}.rs`
- `src/lib/compare-state.svelte.ts`, `src/lib/compare-view.ts`, `src/lib/set-compare.svelte.ts`
- `src/components/FolderCompare.svelte`, `SetCompare.svelte`, `FileCompare.svelte`
- `src-tauri/src/compare/tests/legacy/` (the frozen final-result oracle)

## Steps
1. Write worked examples: messages, state transitions and option-matrix fixtures for identical ids; different ids with equal normalised content; CRLF-only and whitespace-only changes; binary, type conflict and unavailable; budget exhaustion. Check: every interim and final status, every directory, set and filter count and every selection transition has a stated result.
2. Define rollout and lifecycle: endpoints, generation and ordering, completion marker, batch size and rate, selected-file content, fresh write-context behaviour. Check: review of cancellation, consumer-less cleanup, root and ref movement, dirty-editor generation guard, producer failure and backpressure. Trap: closing a session while a producer holds handles; enabling copy because a provisional row exists.
3. Ratify the UX and laziness trade-offs and set per-batch and work limits from the 17 measurements. Amend 19 and 20 with exact types and commands. Check: owner acceptance and an independent review; final fingerprints equal the oracle.

## Done when
- The document has exhaustive state, option and write-eligibility examples.
- The rollout is backward compatible and names a first-useful-render measurement.
- 19 and 20 carry concrete types and commands, or this packet is marked blocked.

## Gates
Documentation only: `git diff --check`. No code gates.

## Stop and report if
- The owner does not accept a normalisation, laziness or filter trade-off.
- Pending totals or write eligibility stay ambiguous.

## Report
Commit sha, the document path, the ratified decisions, the amendments made to 19 and 20.
