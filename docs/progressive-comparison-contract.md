# Progressive comparison contract

Status: proposed by packet 18, 2026-10-08, revision 2 (after review). Packets 19 (backend) and 20 (UI) implement it.
Code references: `src-tauri/src/compare.rs` (written `compare.rs:N`) and its modules, as of main d20faba.

## 1. Purpose

Today a comparison shows nothing until it is finished:
- `Service::prepare` resolves refs, builds the inventory, runs `diff_metadata`, reads and classifies every changed leaf, rolls up folders and computes history. Only then does it return.
- `CompareState.loadAllFiles` then fetches every page before assigning `files`.

On the owner's work PC `compare.prepare` took up to 33.8 s (`plans/2026-10-07/diagnostics-owner-1.md`).

This contract makes a comparison useful as soon as its file list is known, and still ends with exactly today's final result:
- Rows appear early, as **pending**, and can be selected and viewed.
- Status, line counts, renames and history fill in afterwards.
- The final result equals the legacy result byte for byte: same rows, statuses, lines, reasons, renames, sizes and history. The fingerprint is `tests/equivalence.rs` against the oracle in `compare/tests/legacy/`.

## 2. The core rule

**No row is final until the ordered commit (section 5) has processed it.** The ordered commit replays the legacy per-row loop (`compare.rs:1074-1231`) after `diff_metadata` has run, in two lanes. Rows decided without a content read (steps 2-4: untracked repositories, leaf type conflicts, identical ids) commit at once, in any order, because they never touch the byte budget. Rows that reach the budget (step 5 onward) commit strictly in path order. Folder rows commit when their last direct child commits.

As a result:
- the final result is the legacy result by construction;
- an early row is never shown with a status it could lose.

## 3. Terms

| Term | Meaning |
|---|---|
| Session | One comparison, from `comparison_open`. Unchanged. |
| Generation | Bumped by every start, refresh and cancel. Every response and event carries it. Old generations are dropped on both sides. |
| Inventory | All paths on both sides, with per-side `kind`, `size`, `modified_ms`, mode, object id and unavailable reason. Built before any content read (`inventory()`, `compare.rs:1057-1058`). |
| Row | One path in one generation. Its `id` is assigned at inventory and stays the same for that generation. Across generations the `path` is the key. |
| Phase | `pending` or `final`. A final row never changes again in its generation. |
| Hint | What the inventory alone suggests about a pending row. It is shown as a hint, never as a status, and never counted. |

## 4. Session states

```
start ─▶ resolving ─▶ listing ─▶ enriching ─▶ complete
             │            │           │
             └────────────┴───────────┴──▶ failed
cancel / refresh / close ─▶ the generation ends (no further publication)
```

| State | Work | Rows | Writes |
|---|---|---|---|
| `resolving` | Root checks, ref resolution, the single allowed `origin` fetch, object format | none | refused |
| `listing` | Inventory | none | refused |
| `enriching` | Pending rows are published (only if the union is at most 20,000 paths). `diff_metadata` runs, then the 20,000-path check, then the ordered commit | all, pending or final | final rows only (section 9) |
| `complete` | All rows final, summaries final, history final | all | as today |
| `failed` | Session error (see the list below) | frozen, never writable | refused |

Before publishing, listing counts the union of both sides. If it is over `FILE_LIMIT` (20,000), no rows are published: `diff_metadata` still runs so that its error wins, as in legacy, and then the session fails with `limitExceeded`.

The **error order matches legacy**: inventory errors, then `diff_metadata` errors, then `limitExceeded` (more than 20,000 paths), then cancellation, then `history` errors. Because legacy runs `diff_metadata` before the path limit (`compare.rs:1059` before `1066`), the path limit is checked only after `diff_metadata` has finished. An over-limit session stays in `listing` with `totals: None` until it fails, so the UI never shows an empty `enriching` list.

Failures after `listing`:
- `diff_metadata` errors (`gitError`)
- `limitExceeded`
- `cancelled`
- `history` errors

Read failures and line-count failures are **per-row** reasons, as today (`compare.rs:1147-1152`, `text_diff.rs:187-205`). They never fail the session.

`Progress.outcome` maps `problem.kind` to the legacy `RefreshResult` status in any state, exactly as `refresh` does (`compare.rs:1391-1397`). Unknown kinds are returned as `problem`.

## 5. The ordered commit

It runs once per generation, after `diff_metadata`. It walks paths in `BTreeSet` order and keeps one `used` byte counter, as legacy does. For each row it applies the legacy rules in the same order:
1. **Base status.** It is `Unavailable` if either side has a reason, `TypeConflict` if the kinds differ, `LeftOnly` or `RightOnly` for an orphan, and `Same` otherwise. `rename` and `reason` come from `diff_metadata` (`compare.rs:1102-1103`).
2. **Untracked nested repository.** The reason is overwritten with the opaque-repository text. The status becomes `Unavailable` if both sides exist and the status is `Same`. The row commits.
3. **Leaf type conflict** (the base status from step 1 is `TypeConflict` and neither side is a folder; a row with a reason is `Unavailable` and goes to step 5). The row commits as it is. Any row with a folder side goes to step 9 instead.
4. **Identical id.** Both sides have no reason, `oid` is `Some` and equal, `blob_id` and `mode` are equal, and both sides are in the same repository (`safe.path`). The row commits as `Same` with lines `None` and binary `None`, and no read happens.
5. **Pre-check.** If the sum of `entry.size.unwrap_or(0)` over the present sides is greater than `BYTE_LIMIT - used`, the reason becomes "Comparison diff content budget exceeded" and the status `Unavailable`. Nothing is added to `used`. The row commits.
6. **Read.** Read the left side, then the right side. Add `content.len()` to `used` only when a read succeeds. A left error sets the reason and status `Unavailable`, and the right side is not read. A right error keeps the left side's bytes counted. `cancelled` fails the session.
7. **Post-check.** If `used > BYTE_LIMIT`, the reason becomes "Comparison diff content budget exceeded" and the status `Unavailable`, overriding any read error. The row commits. From here on the remaining budget is 0, so every later row with a cost above 0 fails the pre-check, and a later row with cost 0 (size `None`, a gitlink) is read and then fails the post-check.
8. **Classify.** Exactly `compare.rs:1164-1226`: binary detection, raw and display status through `normalized()`, `metadata.lines` before `line_counts`, and `count_result` appending to the reason.
9. **Folders.** A folder row commits after all its direct children have committed. Its status rolls up only if both sides exist and its base status is `Same`:
   - any child `Unavailable` gives `Unavailable`;
   - all children `Same` gives `Same`;
   - otherwise it is `Different`.
   Raw and display roll up independently (`compare.rs:1248-1271`). Its folder-side `size` and `modified_ms` are the legacy formula: the checked sum and the maximum over the direct children's sides (`compare.rs:1273-1295`).
   Note the legacy loop runs in reverse. Children sort after their parent and the roll-up reads only direct children's final values, so committing each folder after its children gives the same result.

Read-ahead is allowed. While earlier rows commit, later rows' reads may already run, in a window of at most 64 rows or 32 MiB. Speculative bytes are discarded and never counted if the row's pre-check or post-check rejects it when its turn comes. Strict path order applies to the budget lane only (steps 5-8); the fixed lane (steps 2-4) and folders (step 9) commit as described in section 2.

The selected file is never committed out of order, because that would change which rows hit the budget. Its content is served at once instead (section 8).

## 6. What a pending row shows

These values are inventory facts. They are final for the row, so they are safe to show early:
- the path and which sides exist;
- `kind`, `size` and `modified_ms` per side. For folder sides, `size` and `modified_ms` are computed bottom-up at listing time with the legacy formula from step 9, so they never change later.

| Hint | When |
|---|---|
| `unavailable` | Either side has a reason. Shown as "Unavailable (checking)", because the budget can still change the reason. |
| `typeConflict` | Kinds differ |
| `leftOnly` / `rightOnly` | Orphan, file or folder |
| `sameId` | Inventory step 4 would match. It commits in the fixed lane right after `diff_metadata`. |
| `opaque` | Either side is an untracked nested repository. It commits in the fixed lane. |
| `changedId` | Both leaves; ids differ, or the files are in different repositories |
| `folder` | Folder present on both sides |

Hint precedence follows the commit steps: `opaque` (step 2 overrides any reason), then `unavailable`, then kind (`typeConflict`, `folder`), then orphan (`leftOnly`, `rightOnly`), then id (`sameId`, `changedId`).

A different object id is never a difference: CRLF-only and whitespace-only edits change the id but can end `Same`. `changedId` reads as "Checking…", never "Different".

## 7. Types, commands and events

Legacy types stay byte-identical: `Snapshot`, `FileRow`, `Summary`, `History`, `RefreshResult`. Pending rows are a separate type with no status and no line fields, so a UI path that ignores the phase cannot show a placeholder status as real.

```rust
#[serde(rename_all = "camelCase")]
pub enum Hint { Unavailable, Opaque, TypeConflict, LeftOnly, RightOnly, SameId, ChangedId, Folder }

#[serde(rename_all = "camelCase")]
pub struct PendingRow { id: String, path: String, left: Option<SideInfo>, right: Option<SideInfo>, hint: Hint }

#[serde(tag = "phase", rename_all = "camelCase")]
pub enum RowUpdate { Pending(PendingRow), Final(FileRow) }

#[serde(rename_all = "camelCase")]
pub enum State { Resolving, Listing, Enriching, Complete, Failed }

#[serde(rename_all = "camelCase")]
pub struct Totals {
    raw: Summary,        // final rows only, counted by the Summary rule: any side that is not a folder (compare.rs:1300-1303)
    display: Summary,
    pending: usize,      // pending rows that the same rule would count; includes file-vs-folder type conflicts
    rows: usize,         // all rows, folders included
}

#[serde(rename_all = "camelCase")]
pub struct Progress {
    id: String,
    generation: u64,
    state: State,
    sequence: u64,           // last update included; pass it back as `after`
    rows: Vec<RowUpdate>,    // updates after `after`, in sequence order
    more: bool,              // more updates are ready now
    totals: Option<Totals>,  // Some from `enriching`
    history: Option<History>,// Some once history is final
    snapshot: Option<Snapshot>, // Some only when `complete`; equal to the legacy snapshot
    outcome: Option<String>, // legacy RefreshResult status for a failure, as refresh maps it
    problem: Option<Problem>,
}
```

Each row is published once as `Pending` and once as `Final`. Sequence numbers start at 1 in every generation.

| Command | Input | Returns | Notes |
|---|---|---|---|
| `comparison_start` | `id`, `options` | `{ id, generation }` | Same checks as `comparison_refresh`: rebind, stale context. It returns at once. The producer is a task owned by the session. |
| `comparison_progress` | `id`, `generation`, `after`, `limit` (1-500) | `Progress` | Returns at most `limit` rows and about 1 MiB serialized. A stale generation returns `staleGeneration`. |
| `comparison_refresh` | unchanged | unchanged | Runs the same producer and waits for `complete`. Results and errors are identical to today. |
| `comparison_files` | unchanged | unchanged | Only when `complete`; otherwise it returns `RefreshRequired`. Old clients never hit this, because their refresh already waits for completion. |
| `comparison_content` | unchanged signature | unchanged | Also allowed during `enriching` for a side that exists in the inventory without a reason. Read-only, `interactive` class. |
| `comparison_commits` | unchanged | unchanged | Only when `complete`, as today. During `enriching`, `Progress.history` gives the counts. |
| `comparison_cancel`, `comparison_close` | unchanged | unchanged | End the generation and stop the producer. |

Event: `CoreEvent::CompareProgress(EventPayload)`.
- Its payload is `{ id, generation, sequence, state }`.
- `kernel/events.rs` `frontend_event` maps it to the webview event name `compare-progress`. Unmapped variants are dropped, so this mapping is required.
- At most 10 events per second per session, coalesced so the latest sequence wins.
- An event is always sent on every state change.
- Events carry no rows: the UI pulls with `comparison_progress`. A slow or hidden UI receives no flood.

## 8. UI behaviour

**Identity.** During `enriching` the UI identifies the session by `{ id, generation }` from `comparison_start`. It does not need a `Snapshot`. Packet 20 rekeys `loadContent`, the file view (`FileCompare.svelte` stale check) and file opening from the folder view (`FolderCompare.svelte`) on this identity. `snapshot` arrives only with `complete`.

**Two modes in `CompareState`.**
- `open()` keeps today's behaviour: wait for `complete`, then load all files. Set compare (`set-compare.svelte.ts`) and the drilldown keep using it, unchanged, in the first release.
- `openProgressive()` is the new path, used by the single-comparison views.
- Set compare shows "Checking N files" only in a later release, after it moves to `openProgressive()`.

**Selection.**
- Any row can be selected during `enriching`. It is keyed by `id` and survives updates.
- Opening a pending row shows both sides' content and the editor's own diff (CodeMirror, packet 37). The header reads "Classifying…" until the row is final.
- New behaviour: after a refresh, the UI re-selects by `path`. Today `loadAllFiles` clears the selection.

**Filters** (`compare-view.ts` `compareRows`):

| Filter | Final rows | Pending rows |
|---|---|---|
| All | all | all, with their hint |
| Differences | `displayStatus != same` | hints `changedId`, `leftOnly`, `rightOnly`, `typeConflict`, `unavailable`, shown as "Checking…" |
| Same | `displayStatus == same` | `sameId` only, shown as "Checking…" |
| Orphans | `leftOnly`, `rightOnly` | hints `leftOnly`, `rightOnly` |

**Totals.**
- The totals bar counts final rows only. It also always shows "N checking" (`Totals.pending`) until `complete`. Pending work is never hidden.
- A provisional total is never shown as final.
- At `complete` the totals equal `Snapshot.raw` and `Snapshot.display`.

**Folders.** Folder rows show their size, their mtime and a pending-children count. They are expandable during `enriching`.

## 9. Writes

Pending rows never authorise a write.
- **Editing.** The editor on a pending row is **read-only** until the row is final. The toolbar shows "Checking…". When the row turns final, the editor calls `file_edit_open` and replaces its buffer with the returned `EditFile.bytes`, so the buffer always matches the ticket's expected bytes. If those bytes differ from what was shown, the editor says "File changed on disk; reloaded". Selection and scroll are kept where the line still exists. Nothing typed is lost, because nothing can be typed while read-only.
  - This reuses today's flow: `file_edit_open` calls `write_context(fresh=true)` (`files.rs:772`), and `write_context` refuses pending rows. So `files.rs` does not change.
- **`write_context` and `copy_source_context`** need the session in `enriching` or `complete`, and the row final. All of today's checks still apply:
  - working tree only;
  - a regular file with no reason;
  - a fresh root check;
  - the Linux root value.
- **`copy_ids`** (single file or folder) needs every row in the scope to be final. That includes rows absent on the source side, which are counted as retained.
  - A single file refuses with "Wait until this file finishes checking".
  - A folder refuses with "Wait until this folder finishes checking".
  - The 128-file limit and destination-only retention are unchanged.
- The dirty-editor guards on `refresh`, `cancel`, `close` and ref change are unchanged.
- A stale generation refuses every write with `staleGeneration`, as today.

## 10. Lifecycle and bounds

- **Ownership.** The session owns its producer task, readers and retained data.
- **Stale publication.** Every publication takes the `sessions` mutex and checks the generation, as `refresh` does at `compare.rs:1380-1384`. That covers every batch of row updates, the history, and every state change. An old generation publishes nothing.
- **Ending a generation.** `cancel`, `refresh` and `close` set the cancel flag, bump the generation and close the readers, as today. `release_sessions` (`lib.rs:106`, on reload and exit) stops every producer.
- **No consumer pause.** The producer is bounded and runs to completion, as legacy `prepare` does. Memory is bounded by `FILE_LIMIT`, the in-flight window, and the retained rows that the session keeps today anyway.
- **Root and ref movement.** Same as legacy: the producer does not rebind during the run. Writes still run their own fresh root check, and the next refresh rebinds.
- **Slot.** The producer holds one `slots` permit for its whole run, as `refresh` does (`compare.rs:1374`). `comparison_content` never waits on permits held by producers: it uses a separate interactive permit pool (at least one slot that producers cannot take).
- **Producer panic.** The session keeps the producer's `JoinHandle`, and a supervisor task awaits it. A `JoinError` becomes `failed` with a `Problem` (`kind: "internal"`). The readers close, no partial `snapshot` is published, and a waiting `comparison_refresh` returns the same error.
- **Bounds:**
  - at most 32 MiB of content in flight per comparison and 64 MiB across all (fixed constants until packet 21's budget module);
  - batches of at most 500 rows or about 1 MiB;
  - at most 10 events per second per session;
  - a read-ahead window of at most 64 rows or 32 MiB.
- **Admission classes.**
  - `interactive`: inventory, `comparison_content` and ref resolution.
  - `enrichment`: reads, classification, counts, `diff_metadata` and history.
  - Packet 19 sets the rules: enrichment never takes the last free slot.
- **Traps to avoid:**
  - closing a session while a producer holds reader handles, without cancelling first;
  - enabling save or copy because a pending row exists;
  - committing a selected row out of order.

## 11. Worked examples

E means `normalizeEol` and W means `ignoreWhitespace`. "Pending" is what is shown at listing time. "Final" is the committed result. Every final cell must equal what the legacy oracle returns for the same fixture. Packet 19's matrix test asserts this for all four combinations of E and W.

| # | Case | Pending | Final, E=0 W=0 | Final, E=1 | Final, W=1 |
|---|---|---|---|---|---|
| 1 | Same id, same mode, same repository | `sameId` | Same/Same, lines none, binary none | same | same |
| 2 | Same id, different repositories | `changedId` | Same/Same, lines 0/0 | same | same |
| 3 | CRLF vs LF only | `changedId` | Different/Different, lines n/n | raw Different / display Same, display 0/0 | raw Different / display Different (W keeps `\r`) |
| 4 | Indentation only | `changedId` | Different/Different | Different/Different | raw Different / display Same, display 0/0 |
| 5 | Mode change only | `changedId` | Different/Different, 0/0 | same | same |
| 6 | Binary, different | `changedId` | Different/Different, binary true, lines none | same | same |
| 7 | File vs folder | `typeConflict` | TypeConflict; rename from `diff_metadata` if any | same | same |
| 8 | Left only, text | `leftOnly` | LeftOnly, 0 added / n removed | same | same |
| 9 | One working-tree side unreadable (read failure, broken gitlink) | `unavailable` | Unavailable with the side's reason, or the budget reason if the budget is exceeded | same | same |
| 10 | Untracked nested repository on both sides | `opaque` | Unavailable, opaque-repository reason | same | same |
| 11a | Budget pre-check: a 40 MiB file after 30 MiB used | `changedId` | That row is Unavailable (budget). Smaller later rows are still read and classified normally | same | same |
| 11b | Budget post-check: the read pushes `used` over 64 MiB | `changedId` | That row is Unavailable (budget). Every later row that needs a read is Unavailable (budget). Rows with no read (steps 2-4) are unaffected | same | same |
| 12 | Gitlink changed | `changedId` | Different, lines none | same | same |
| 13 | Folder on both sides | `folder` | After its children: Unavailable if any child is Unavailable, Same if all are Same, otherwise Different. Raw and display roll up separately | follows children per option | follows children per option |
| 14 | Working tree vs HEAD, same repository | any | Every row carries the reason "Working-tree rename metadata unavailable; clean filters are not executed" (`text_diff.rs:220-223`); lines from `line_counts` | per E | per W |

Unmerged index and non-UTF-8 paths are not row cases. They fail the whole comparison as `unavailable`, before listing, exactly as today.

**Selection during `enriching`:**

| Action | Result |
|---|---|
| Select a pending row | Selected. Content opens read-only. The header reads "Classifying…" |
| The row turns final while selected | Selection and scroll are kept. The status and lines appear, and the editor becomes editable |
| A filter hides the selected row | As today: the row is hidden and the details drawer stays |
| Refresh | New generation. The UI re-selects by path when that path is listed (new behaviour) |
| Close or cancel with unsaved edits | The existing guard asks first |
| Copy a file or folder with pending rows | Refused with the "Wait until…" message |

## 12. Rollout

1. **Packet 19.** Add the producer, the new types and the new commands. `comparison_refresh` runs the same producer and waits, so legacy output is unchanged; the equivalence tests prove it.
2. **Packet 20.** Add `openProgressive()` to `CompareState` for the single-comparison views. `open()` keeps the wait-for-complete behaviour for set compare. A one-release fallback switches back to `comparison_refresh` for diagnosis.
3. **Later release.** Set compare moves to the progressive path and shows "Checking N files". The legacy commands stay registered for the equivalence tests and the benchmark.

## 13. Measurement

First-useful-render is the time from the compare action to the first rendered, selectable row.

| Timer | Start | Stop |
|---|---|---|
| `ui.request` (exists) | compare action | `comparison_start` returns |
| `compare.listed` (new, backend) | start | the state becomes `enriching` |
| `ui.first-row` (new) | compare action | the first frame with a selectable row |
| `ui.complete` (exists) | compare action | `complete` is rendered |

Measure with caches empty and prewarm off in three places:
- the owner's Windows PC, through a diagnostics export;
- the Windows CI fixture;
- the 20,000-file Linux fixture.

Targets:
- `ui.first-row` is at most 25% of `ui.complete` on the 20,000-file fixture.
- `ui.first-row` is under 2 s on the owner's largest repository.
- Final fingerprints equal the legacy ones.

## 14. Open decisions for the owner

1. **Laziness.** Every changed file is still read in full to classify it. The gain is seeing and opening files early, not less total work. Accept?
2. **Differences filter.** Files whose content id changed show as "Checking…" under Differences, and some will end up Same once line endings or whitespace are ignored. Accept, or show them only under All?
3. **Editing.** A file opens read-only until it is classified, which takes seconds, then becomes editable in place (reloaded from disk at that moment). Accept?
4. **Copy.** Copying a file or folder is refused until it has finished checking. Accept?
