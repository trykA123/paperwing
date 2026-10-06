# Packet 04 — responsibility-based presentation components

**Status:** Proposed; requires approval. **Weight:** Standard. **Depends on:** accepted 01 and 03.
**Read first:** `FileCompare.svelte`, `SetView.svelte`, `Settings.svelte`, `VirtualList.svelte`, `src/App.svelte`, CSS selectors and the working plan's native verification rules.

## Outcome and scope
Extract cohesive presentation from the largest workflow components without relocating lifecycle ownership or changing DOM/classes/behavior. No visual redesign, new animations, new UI library or platform gating in this packet.

## Requirements and invariants
- R1: FileCompare, SetView and Settings delegate coherent view sections with explicit data/callbacks.
- R2: User interactions, layout, shortcuts, validation, focus and selection match baseline.
- I1: Monaco/models/tickets/dirty guards remain FileCompare-owned; tab mount/teardown semantics do not change.
- I2: Settings draft survives section changes; SetView owns paging/edit/picker state above virtual rows. Grid children/row height and CSS cascade are preserved.

## Evidence
- [FACT] FileCompare combines editor lifecycle with toolbar/footer presentation; hidden comparison/file tabs stay mounted in App.
- [FACT] Settings draft is parent-owned while its sections are conditional; moving it into a conditionally mounted child loses unsaved state.
- [FACT] SetView grid/snippet structure and VirtualList row sizing are contractual layout details.
- [DECISION] Use explicit props/callbacks, not broad context access or a replacement generic controller.

## Steps
- P1 [BATCH]: Extract FileCompare toolbar/endpoints/footer into proposed `components/file-compare/{Toolbar,Endpoints,Footer}.svelte`. Keep original classes/semantic DOM and parent actions.
  - Depends on: none. Location: FileCompare markup; `save`, `guard`, `copy`, editor effect remain in parent.
  - Verify: `bun run --bun check`; `bun test src/lib`; native first editor load, hunk navigation/copy, keyboard shortcuts and dirty close.
  - Fails if: a child creates/disposes models, tickets or guards.
- P2 [BATCH]: Extract local/status cell presentation into proposed `components/set/{LocalCell,StatusCell}.svelte` with explicit item/status/action props. Keep grid children and virtual row contract identical.
  - Depends on: P1. Location: SetView local/status snippets and callbacks.
  - Verify: native scrolling, paging, selection/range selection, inline folder rename, ref picker and push-button eligibility at baseline row heights.
  - Trap: adding wrappers that become extra grid children or storing row state in recycled components.
- P3 [BATCH]: Extract appearance/cloning and source form presentation into proposed `components/settings/{AppearanceSection,CloningSection,SourceForm}.svelte`. Bind to parent-owned draft/token/error/validation state; do not change save/cancel behavior.
  - Depends on: P2. Location: Settings section markup and draft handlers.
  - Verify: unsaved source edits survive section changes, cancel discards only draft, token errors and organization selections persist appropriately; restart appearance settings.
- P4 [ISOLATE]: Review flattened DOM/class/ownership changes and inspect native Windows/Linux rendering of controls, dialogs, focus/disabled states and hidden tab lifetimes.
  - Depends on: P3. Location: extracted components, parent effects, global selectors.
  - Verify: `bun run --bun check`; `bun test src/lib`; `bun run --bun build`; side-by-side baseline/native screenshots and interaction matrix.
  - Fails if: SSR-only snapshots are used as proof of effects/focus/editor cleanup.

## Acceptance
- A1 (static + fixtures): cohesive children, unchanged action ownership and passing frontend gates → R1/I1/I2.
- A2 (native owner-observed): editor/virtual-row/settings interactions and focus/DOM/layout match baseline → R2/I1/I2.

## Stop and rollback
Stop on a changed interaction, draft/tab lifetime or DOM/cascade. Restore only owned extraction changes. New Linux behavior and progressive UI delivery are later packets, not convenient additions here.

## Revision log
- 2026-10-02: Proposed presentation-only extraction.
