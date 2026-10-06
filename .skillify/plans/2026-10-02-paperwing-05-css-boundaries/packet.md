# Packet 05 — split CSS without changing its cascade

**Status:** Proposed; requires approval. **Weight:** Standard. **Depends on:** accepted 01.
**Read first:** `src/app.css`, `src/main.ts` and the working plan's baseline/native rules.

## Outcome and scope
Replace the 5,486-line stylesheet with an ordered entry and cohesive chapters while preserving its exact flattened cascade. Out: selector cleanup/regrouping, deduplication, design-token changes, layers, scoped-style conversion, responsive redesign and animation changes.

## Requirements and invariants
- R1: Smaller named CSS chapters are imported in a recorded stable order.
- R2: Flattening the chapters reproduces original rules/declaration order and rendered behavior.
- I1: Later overrides retain priority; fonts, disabled/focus states, editor/set layouts, light/dark and reduced-motion behavior do not change.

## Evidence
- [FACT] Later `.editor-count` and `.local` declarations intentionally override earlier equal-specificity rules in `app.css`.
- [DECISION] Split contiguous source chapters, not all matching selectors; avoid interleaving/reordering media and override rules.

## Steps
- P1 [BATCH]: Record chapter boundaries and a normalized flattened-source fingerprint plus native screenshots/representative computed styles.
  - Depends on: none. Location: `app.css` and proposed `scripts/testing/css-order.ts`.
  - Verify: `bun run scripts/testing/css-order.ts --self-test` (proposed); baseline light/dark/focus/disabled/reduced-motion observations.
- P2 [BATCH]: Move one contiguous chapter at a time into proposed `src/styles/{comparison,base,settings,controls,overrides}.css`, adjusting names/splits only to actual contiguous boundaries. Keep `app.css` an ordered `@import` entry.
  - Depends on: P1. Location: original stylesheet chapter ranges; `main.ts` keeps the same entry import.
  - Verify after each chapter: flatten-order self-test; `bun run --bun build`; inspect compiled CSS ordering.
  - Trap: collecting all comparison rules into one file when later comparison overrides originally followed other chapters.
- P3 [ISOLATE]: Compare production native rendering and computed style samples with baseline; retain a complete ordered chapter manifest.
  - Depends on: P2. Location: stylesheet manifest and native fixtures.
  - Verify: `bun run --bun check`; `bun test src/lib`; production build; native baseline-sized and narrower supported windows on Windows/Linux, light/dark and reduced-motion.

## Acceptance
- A1 (static + fixture): flattened rule/declaration order matches baseline and chapter ownership is clear → R1/R2/I1.
- A2 (native owner-observed): representative computed styles/screenshots and focus/disabled/editor/set layout agree → R2/I1.

## Stop and rollback
Stop on reordered/import-hoisted rules, missing font/assets, changed specificity or visual mismatch. Restore stylesheet entry/chapters only; preserve all unrelated UI changes. A passing build is not visual acceptance.

## Revision log
- 2026-10-02: Proposed order-only CSS decomposition.
