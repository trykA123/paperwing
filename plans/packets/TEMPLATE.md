# NN — Title

Status: ready | blocked by NN | in progress on `<branch>` | partly done
Platform: Windows first, Linux parity (say if one platform only)
Size: S (hours) | M (a day) | L (several days)
Role: api-builder | ui-builder | both (backend first)

## Goal
Two or three sentences: what the user can do afterwards that they cannot do now.

## Already done
What exists in the code today, with file paths. Agents start from this, not from scratch.

## Decisions
Final choices. Do not reopen them; stop and report if one turns out to be wrong.

## Scope
- Do: ...
- Do not: ...

## Read first
Repo-relative files and docs, most important first.

## Do not touch
Files owned by other in-flight work, if any.

## Steps
1. One change. Check: the exact test or command that proves it.
2. ...

## Done when
Observable, testable outcomes. Windows behaviour named explicitly.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (UI changes; `--write` regenerates `src/styles/order.json`)
- `cd src-tauri && cargo test --offline` (set `PAPERWING_TEST_TMP` to an ext4 directory, never `/tmp`)
- `rustfmt --check` and clippy on touched Rust files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
Conditions that mean the plan is wrong or a decision is needed.

## Report
Commit sha, files changed, each step's check result, gate results, anything skipped.

## New-integration checklist (providers only)
1. Which capabilities does it offer?
2. Which credentials and config does it need?
3. Can it be disabled completely (no worker, no polling, no traffic, no cache)?
4. Polling or event-driven?
5. In-process or isolated worker?
6. What persists in SQLite?
7. What is cache only?
8. What happens when it fails or dies?
9. Can another provider replace it?
10. Which events does it produce and consume?
