# Packet 18 — Progressive comparison: contract and UX (design only)

| | |
|---|---|
| Status | Proposed. Needs owner approval. Design gate for 19 and 20. |
| Weight | Heavy (public contract). No product code in this packet. |
| Depends on | Accepted 01 (baseline), 02 (comparison modules), 17 (cold path). |
| Primary platform | Windows (work use). The contract is platform-neutral. |
| Read first | Comparison data types and prepare/files/content/commits/write-context commands; CompareState; `compare-view.ts`; FolderCompare, SetCompare, FileCompare; final-result fixtures. |

## Goal
Agree, in writing, how a comparison shows useful rows before it is finished, while ending with exactly today's final result. Packets 19 (backend) and 20 (UI) implement this contract.

## Why
- `prepare` classifies normalized bytes before anything is published, and `compare-view.ts` filters on the final status.
- `loadAllFiles` collects every page before showing anything, and FolderCompare stays in "loading" until then. Batching in the UI alone does not remove the backend barrier.

## Scope
- **In:** data types, states, messages, UX rules for pending rows, rollout plan, first-useful-render metric.
- **Out:** implementation, cache, visual redesign.

## Requirements
- **R1** — Specify stable row IDs per generation; provisional vs final status; unavailable and binary classification; progress, completion and error states; counts and history; directory and set summaries.
- **R2** — Specify Same/Differences filters, selection and totals while normalization is pending, and when content, edit and copy are allowed before completion.
- **R3** — Specify a backward-compatible rollout: batching, backpressure, cancellation, session cleanup and a measurable "first useful render".
- **I1** — A different object ID is not a final difference after normalization. Pending rows and counts never pose as final results and never authorize writes.
- **I2** — Final semantics, limits, destination-only retention and dirty-editor guards stay unchanged. Strict laziness for unopened files is not promised where exact normalized classification needs reads.

## Decisions (proposed, ratify in step 3)
- **D1** — Show a clearly marked pending inventory early. Supported rows can be selected and opened. Classification, counts and history fill in incrementally.
- **D2** — Same/Differences totals exclude unclassified rows and show an explicit "pending" group with its count. Pending work is never silently hidden.
- **D3** — Additive endpoints (candidate `comparison_start` / `comparison_progress`); legacy commands keep waiting for final results.
- **D4** — Background enrichment runs in the `enrichment` priority class defined in packet 19 (R4); user actions always come first.
- **D5** — Memory and batch limits come from the app-wide budget in packet 21.

## Steps
1. **Write concrete examples:** messages, state transitions and option-matrix fixtures for identical object IDs; different IDs with equal normalized content; CRLF- and whitespace-only changes; binary, type conflict and unavailable; budget exhaustion.
   - Where: proposed `docs/progressive-comparison-contract.md`.
   - Check: every interim and final status, every directory, set and filter count, and every selected-row transition has a stated expected result.
2. **Define rollout and lifecycle:** endpoints, stable IDs, generation and ordering, completion marker, batch size and progress rate, selected-file content and fresh write-context behaviour.
   - Check: independent review of cancellation, consumer-less cleanup, root and ref movement, dirty-editor generation guard, producer failure and backpressure.
   - Trap: closing a session while a pending producer still holds handles, or enabling copy just because a provisional row exists.
3. **Ratify the UX and laziness expectations**, set milestones and metrics, and set per-batch and work limits from the baseline. Amend 19 and 20 with the exact types, commands and checks.
   - Check: owner acceptance plus independent review; final fingerprints remain those characterized in 01.

## Done when
- **A1 (static and specified fixtures):** exhaustive state, option and eligibility examples. Covers R1, R2, I1, I2.
- **A2 (static):** compatible rollout, lifecycle, backpressure and a named first-useful-render measurement. Covers R3, I1, I2.
- **A3 (owner-observed):** the contract is accepted and 19/20 have concrete handoffs, or the packet is marked blocked. Covers R1–R3, I1, I2.

## Authority and rollback
The owner decides interim presentation and public contracts. One documentation writer; the main session integrates. No production data is touched. Rollback discards only the proposed contract and handoffs; legacy behaviour is untouched.

Stop on unaccepted normalization, laziness or filter trade-offs, ambiguous pending totals or unclear write eligibility. Do not start 19 or 20 until this gate passes.

## Revision log
- 2026-10-02: Proposed progressive contract; the exact protocol is gated, not chosen silently by an executor.
- 2026-10-05: Added D4 (enrichment priority class from 19) and D5 (app-wide budget from 21). Rewritten in plain format.
