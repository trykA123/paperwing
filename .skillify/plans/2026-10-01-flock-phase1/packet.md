# Flock Phase 1 Approval Packet

Archive: historical design record, not current implementation authority. Referenced prototypes, reviews, and runtime evidence are not included in this source-only copy. Historical commit identity is intentionally omitted; source locations and status describe the original planning snapshot.

Status: P3 implementation and worker checks complete; independent review pending. Phase: Review P3. Weight: Heavy. Verbosity: Concise.
Ownership: staged worker, independent reviewer after every part; user owns scope,
dependency/security approval, filesystem mutation authority and final acceptance.
Latest user approval authorizes P3 ONLY after the P1/P2 checkpoint on
`paperwing-v2-phase1`, at a historical local checkpoint,
Bun 1.4.2, focused checks and disposable real Git/Tauri IPC validation.
Preserve P1/P2, user edits, both settled Paperwing design files, installed assets,
the untracked prototype and the alternate plan folder. No further commits/push,
new dependencies, production build, P4-P8, rename/palette integration or icon work.
Stop after P3 evidence for independent read-only review and user acceptance;
worker checks do not grant independent approval or next-part authority.

## Outcome And Scope

Implement the approved `prototype/flock-v2.html` scenes 1 (set and bulk actions),
4 (folder compare), 5 (file compare and editable working tree), and 6 (whole-set
compare), using Tauri 2, Rust, Svelte 5 runes, TypeScript and Vite 6, Windows first.
Prototype layout, visual language and interactions are the design authority; use
real data, not its canned names, counts, timestamps or command strings.
Scenes 2, 3 and 7 remain Phase 2. No push, new-branch, stash mutation, commit,
history-graph workspace, blame workspace, merge or conflict-resolution actions.
Branches, remotes, tags, stashes and submodules are real read-only tree entries.
Prototype report/patch exports and history/blame affordances are visibly deferred,
not successful-looking toast mocks. The scene-4 Commits toggle is a read-only
left/right unique-commit list, not the Phase 2 history graph.

## Evidence

- FACT: `src/lib/state.svelte.ts:46` defaultWorkspace plus `:93` init merge
  saved UI data with defaults and migrate the former org layout. Destinations
  are derived by segments/dest; duplicate clones have separate SetItem IDs.
- FACT: `src/lib/api.ts:39` Workspace stores sets, sources' references, clone
  settings, columns and appearance; RefKind has branch/tag/commit, no working tree.
- FACT: `src/App.svelte:49` dispatches set/org/search/settings in a fixed shell;
  `:28` autosaves workspace and `:21` refreshes local state on focus.
- FACT: `src/components/SetView.svelte:82` picker application and `:132` bulk
  actions already preserve per-repo coverage, fast-forward pull and switch flows.
  `src/components/RefPicker.svelte:119` implements grouped refs and commit rows;
  reuse these behaviors without changing checkout refs to include working tree.
- FACT: `src-tauri/src/git.rs:9` only constructs processes; `:88` ls_remote,
  `src-tauri/src/local.rs:26` status_of and `src-tauri/src/clone.rs:86` run
  execute/capture separately. Clone streams stderr and suppresses stdout.
- FACT: `src-tauri/src/clone.rs:42` validate checks refs and absolute destination,
  not root containment. `:238` reclone renames to a timestamp backup. New write
  safety must not assume this is a containment or crash-recovery abstraction.
- FACT: `src-tauri/src/settings.rs:19` stores workspace as serde_json::Value;
  `:76` settings saves through a temporary file and rename, with no file undo.
- FACT: `src-tauri/src/lib.rs:33` owns command registration. Current custom IPC
  commands accept paths; no compare/session/mutation command exists.
- FACT: `src-tauri/tauri.conf.json:26` CSP has no worker-src; devCsp is null.
  `vite.config.ts:8` fixes port 1420; existing capabilities are core/dialog/opener.
- FACT: `package.json:6` has check, build and tauri scripts, no test script or
  Monaco dependency. Source search found no Rust test attributes or JS test suites.
- FACT: prototype scene mapping is at `prototype/flock-v2.html:1221`, shell at
  `:711`, folder compare at `:877`, file compare at `:949`, set compare at `:966`.
  Prototype set summary uses simulated three-dot diff; do not copy that algorithm.
- OBSERVED: read-only `rtk git status --short` showed only untracked
  `prototype/flock-v2.html`. Preserve it and do not stage it implicitly.
- OBSERVED: workspace instruction-file searches found no AGENTS.md, CLAUDE.md
  or instruction files. Loaded shapeify; its Heavy persistence rule requires this
  planning-only file under .skillify, within the user's stated exception.
- ASSUMPTION: Git for Windows, MSVC and WebView2 are available at implementation
  time. Worker verifies versions then; no runtime or build health is claimed now.

## Requirements And Invariants

- R1: Dynamic tabbed shell, adjustable/toggleable panes, status, light/dark/system,
  scoped palette and scene 1 preserve all existing workflows and saved data.
- R2: Every Git invocation has Activity identity, context, argv, output, timing,
  exit/cancellation state; tree displays actual refs/remotes/stashes/submodules.
- R3: Validated read-only compares support same/cross-repo branch/tag/SHA/working
  tree endpoints, full inventories, unchanged files, orphans and honest metadata.
- R4: Scene 4 tree, endpoint pickers/swap, selection/details, filters, exclusions,
  normalization rules and unique-commit lists operate on backend results.
- R5: All introduced file reads/writes and changed clone destination handling
  enforce containment and metadata protection; saves/copies are recoverable.
- R6: Scene 5 uses locally bundled Monaco workers, real diff/navigation/overview,
  inline/side modes, hidden unchanged, whitespace options and working-tree editing.
- R7: Directional hunk/file/folder copy, recovery and scoped shortcuts share
  command handlers and never mutate an immutable endpoint or silently delete files.
- R8: Scene 6 compares every set item independently, including duplicate clones,
  reports missing/unavailable per repo, aggregates honestly and drills into scene 4.
- I1: Keep clone/fetch/pull/switch, set CRUD, duplicate/folder/layout/paging,
  org/manual-source browsing, search/favorites/settings/tokens, VS Code opening,
  ref-picker coverage/keyboard/commit graph and appearance behaviors intact.
- I2: Resolve roots on backend; no traversal, Windows alias/reparse escape,
  metadata access/write, stale write, or implicit destructive operation.
- I3: No replacement without verified persisted recovery; failed/conflicting
  operation preserves user bytes, or supplies an explicit tested recovery record.
- I4: No shell command strings or credentials in Activity, errors or persistence.
  Bounded resources and cancellation apply; immutable comparisons do not checkout.
- I5: Phase 2 actions remain unavailable; user approval and each review gate are
  mandatory. No scope expansion or dependency changes; approved local checkpoint
  branch `paperwing-v2-phase1` only, with no further commit or branch authority.

## Ordered Parts

All new files below are proposed targets, not existing evidence. Each part leaves
a green, committable boundary; do not start the next until user-reviewed signoff.

### P1 [BATCH] Shell And Existing Features (R1, I1, I5)

Targets: `src/App.svelte`, `src/app.css`, `src/lib/state.svelte.ts` defaultWorkspace,
init/View, `src/lib/api.ts` Workspace; `Sidebar.svelte`, `RightPanel.svelte`,
`SetView.svelte`, `Icon.svelte`; new `src/components/Tabs.svelte`,
`CommandPalette.svelte`, `src/lib/commands.ts` and `src/lib/workspace.ts`.
Use discriminated tabs keyed by set/item IDs, not repository names or global active
set alone; per-tab context must survive switching sets. Retain RepoList, Settings,
Pager, VirtualList and RefPicker as existing behavior owners. Pane sizing/toggle,
focus, context menus, keyboard palette selection and dirty-tab close guards are
shared shell behavior. Palette initially exposes only implemented commands.
Migration adds versioned optional layout defaults; keep old fields/IDs and unknown
saved properties, never persist editor buffers, activity, credentials or transient
sessions in workspace autosave. Do not change defaultWorkspace root opportunistically.
Check A1: old settings fixture loads/saves twice without loss, duplicate folders
stay distinct, all legacy flows work, shell matches scene 1 in both themes at
1440x900 and minimum 1100x640; no hidden actions become enabled.
Failure: lost settings, stale tab/set context, clipping or changed clone semantics.

### P2 [ISOLATE] Git Execution, Activity And Tree (R2, I1, I4, I5)

Progress: implementation preserved; final independent `reviews/S2-review.md`
grants technical approval. User checkpointed P1/P2 and explicitly authorized P3.
19 original live observations plus 9 recovery observations remain historical
evidence. See `evidence/S2-report.md` and the `recovery` section of
`evidence/S2-live-results.json`; no historical live result is relabeled as P3 proof.

Depends on P1. Targets: `src-tauri/src/git.rs` git/ls_remote, `clone.rs` run/
check_origin, `local.rs` status_of, `lib.rs` registration; `api.ts`,
`state.svelte.ts`, `Sidebar.svelte`; new `src/components/ActivityDrawer.svelte`.
Replace the constructor-only boundary with typed buffered/streaming execution
through one owner, draining BOTH stdout/stderr without deadlock while preserving
clone progress. Every command, including probes, gets events plus a bounded backend
snapshot so startup events are not lost; clear finished cannot erase running jobs.
Use IDs and monotonic sequence numbers, cancellation/timeouts and exit semantics
(expected probe/no-index statuses are not spurious failures). Preserve full useful
output; explicitly mark truncation for large content rather than freezing the UI.
Redact configured secrets, URL userinfo/query credentials and sensitive headers
before event/error/storage sinks, across streamed chunk boundaries. For arbitrary
blob output log safe metadata/size or a labeled redacted bounded preview, not raw
file contents that might be secrets. No persistent Activity log by default.
Tree reads use for-each-ref, remote enumeration, stash list and gitlinks/.gitmodules
as inert data; no submodule update/init or branch/stash mutations. Refresh on demand
and focus, invalidate after existing operations; no polling/network loop.
Check A2: runner fixture captures both streams, failure/cancel/redaction and clone
progress; live clone/fetch/status/refs each appear once with output and clear works.
Failure: any Git call bypasses runner, secrets leak, progress hangs or clone regresses.

### P3 [ISOLATE] Read-Only Compare Contract And Engine (R3, I2, I4)

Progress: worker implementation and required gates complete on the approved
checkpoint branch, with no new commit. Final Rust suite: 16 pass (5 Git, 9 compare,
2 paths); Bun suite: 14 pass / 71 expectations; Bun check: 0 errors / 0 warnings;
all-targets Clippy with warnings denied: clean. Final-source Windows junction
refusal and 8 real Tauri IPC protocol proofs pass; disposable HEAD/index/worktree
fingerprints are unchanged. Physical symlink creation is privilege-blocked;
read containment is not claimed race-proof. See `evidence/S3-report.md` for exact
source/runtime identity, limitations and classified corrections. Next owner:
independent reviewer. No P4 or self-signoff.

Depends on P2. Targets: new `src-tauri/src/compare.rs` plus `paths.rs`, existing
`git.rs`, `lib.rs`, `src/lib/api.ts`; new `src/lib/compare.svelte.ts`.
Commands: open/refresh/close comparison, enumerate files, load selected content,
and list unique commits. Pass structured endpoint/ref/path DTOs; server validates
registered set-item/root identity and returns opaque session/file IDs. A supplied
path is not authority. Keep CompareRef separate from checkout RefKind.
Branches/tags must pass check-ref-format and resolve explicit namespaces; SHA must
be hex, unambiguous and peel to a commit using rev-parse --verify --end-of-options.
Permit HEAD as a defined UI alias, never arbitrary revision expressions. Working
tree is a discriminant, not a ref string. Never checkout to compare. If an object
is missing, fetch origin once per affected repo/request, coalescing concurrent
requests, then resolve again; do not loop, unshallow automatically or auto-clone.
Distinguish invalid/missing-left/missing-right/unavailable/not-cloned/network-error.
Use args, option separators, sanitized environment, no external diff/textconv.

Inventories: ls-tree -r -z -l for each committed tree, show of resolved object IDs
for blobs, ls-files -z --cached --others --exclude-standard plus actual safe reads
for working tree (including staged, unstaged, untracked and tracked deletions).
Ignored files excluded by default; exclusion patterns never grant path access.
Union relative paths yields same/different/left-only/right-only/type-conflict;
name-status -z and numstat -z optimize same-repo two-endpoint diff, NOT A...B.
Cross-repo compares union inventories and bytes/content hashes, then numstat via
safe temporary materializations and git diff --no-index when needed; diff exit 1
means differences. Do not require a shared object database or matching hash format.
Treat rename metadata explicitly but classify/copy by actual paths; no rename
deletion. Binary counts are unavailable, not zero. Keep raw status separately from
normalized display status (whitespace/EOL), matching filters/counts to the latter.
Normalize only comparison; never rewrite original bytes as a side effect.
Commit blob sizes are real; no filesystem mtime for a historical blob. Filesystem
dates are real and optional; don't substitute commit dates as Modified silently.
Folders aggregate children, with no synthetic empty tracked folders. Symlinks are
inert link text; gitlinks are opaque entries, not recursive nested repos. Reject
external/unreadable working-tree links and report why. I2 checks all ancestors,
Windows drive/UNC/device paths, ADS, case/trailing-dot aliases and .git plus actual
git/common metadata locations; new nonexistent leaves validate through parent.
rev-list --left-right --count and unique lists apply only to compatible history;
cross-repo unrelated or shallow-incomplete ancestry is N/A, not fabricated ahead.
Working-tree history uses labeled HEAD ancestry, not invented uncommitted commits.
Bound jobs/content, lazy-load blobs, ignore stale generations, offer cancel.
Check A3: Rust temporary-repo fixtures cover SHA/branch/tag/HEAD/WT, invalid refs,
one fetch attempt, unavailable clone, binary/rename/Unicode paths, same/orphan,
CRLF/whitespace, cross-repo unrelated and shallow ancestry, staged/untracked/deleted,
and read containment. Refresh never changes HEAD/index/worktree bytes.
Failure: identical files disappear, untracked missing, false history counts,
unsafe reads, checkout, retry loops, ambiguous SHA silently chosen.

### P4 [BATCH] Folder Compare UI (R4, I2, I5)

Depends on P3. Targets: new `src/components/FolderCompare.svelte`,
`CompareEndpointPicker.svelte`; `RefPicker.svelte` via additive adapter only,
`VirtualList.svelte` only if needed, `RightPanel.svelte`, `Tabs.svelte`,
`compare.svelte.ts`, `App.svelte`, `app.css`, `commands.ts`.
Scene 4 endpoint repo/ref selectors, swap, synchronized trees, expand/collapse,
All/Differences/Same/Orphans, filename/glob filter, exclusions, real size/date and
details use backend results. Support cross-repo selections and either WT side.
Unique-commit list uses P3, with N/A when inappropriate. Debounce/cancel and protect
selection from out-of-order results. Double-click opens a read-only file preview
until P6 replaces that same seam with Monaco; copies remain disabled until P7.
Check A4: live scene 4 same/cross-repo, each filter including same and orphan,
swap, loading/error/empty, keyboard open and snapshot refresh match prototype.
Failure: mismatched side identity/counts, fake timestamps, stale result or mock actions.

### P5 [ISOLATE] Recovery And Windows Write Boundary (R5, I1, I2, I3, I4)

Depends on P3/P4. Targets: `src-tauri/src/paths.rs`, new `files.rs`, `lib.rs`,
`api.ts`, `clone.rs` validate/run_job and clone options in state/api.
Typed read/save/copy/undo operations bind to current root/item/session and expected
content fingerprints. Serialize writes per destination and coordinate with
clone/fetch/pull/switch, root changes and dirty editor sessions. Revalidate source,
target and repo identity at execution; foreign modifications produce conflict,
never last-writer-wins. Protect .git files/directories and real metadata locations.
Reject unsupported reparse points, symlinks, junctions, device/ADS/alias paths,
special files and nested-repo writes. Canonical-prefix checks alone are NOT proof
against Windows junction races: implement handle-based ancestor/target validation
with handles preventing rename/reparse substitution, or keep affected writes
disabled. Atomic replacement avoids modifying shared hard-linked original bytes.
Use a small Windows platform dependency only under the dependency approval gate;
prefer an existing adequate API, do not implement unsafe Windows FFI casually.

Before replace: durable unique journal + byte backup + checksum + target metadata;
verify backup, create sibling temporary with create-new, write/flush/sync, then
Windows atomic replace (or no-replace move for absent target). No delete-then-rename
fallback. Failure before commit leaves target untouched; after commit report exact
journal state and reconcile at restart. Preserve supported encoding/BOM/EOL/mode;
unsupported encodings/binary editor save are disabled, whole-file byte copy remains
possible. Failed backup, disk-full, locked file or stale content refuses operation.
Copy operation rollback is per-file journaled; multi-file directory operations
are NOT falsely presented as atomic. Failed partial batch shows completed/pending
paths and recovery; retry revalidates. Undo compares current post-write fingerprint
and refuses to overwrite subsequent user work. Created files are removed on undo
only when still exactly the recorded bytes; otherwise retain and report conflict.
Integrate root containment into existing clone paths without changing checkout /
fast-forward policy. Existing reclone remains explicit confirmed backup-then-clone,
uses collision-proof backup names and retains backup on failure; no silent deletion
of old or partial clones. Existing local reads still work for supported safe paths.

Proposed recovery decision: persisted private app-data byte backups with a durable
journal PLUS bounded session undo handles pointing to those backups. No repo .bak
files for each editor save (they pollute untracked inventories); keep legacy reclone
directory backups. A byte/count storage cap stops new writes, never evicts needed
recovery automatically. Explicit confirmed cleanup only for finalized, unreferenced
records; pending/conflicted journals are protected. Recovery after restart remains
available even though the quick undo stack is session-scoped. Disk use, private
source-content retention and backup location must be disclosed at approval.
Check A5: Windows temporary-root tests for traversal/.git/aliases, symlink/junction
escape and substitution, stale writes, locks, disk/backup failure injection,
crash at each journal boundary, undo conflict, partial directory copy and restart
restore; verify target/recovery checksums and outside-root sentinel unchanged.
Failure: unverified backup, outside-root access, user bytes lost, unsafe race,
metadata mutation, delete fallback or irreversible partial copy. No UI writes yet.

### P6 [ISOLATE] Monaco File Compare And Save (R6, I2, I3, I4)

Depends on P5. Targets: `package.json`, `package-lock.json`, `vite.config.ts`,
`src-tauri/tauri.conf.json`; new `src/lib/monaco.ts`,
`src/components/FileCompare.svelte`; `compare.svelte.ts`, `app.css`, `commands.ts`.
Add monaco-editor after approval, pin lockfile, bundle editor/language workers
locally through Vite worker imports (no CDN). Explicit worker-src 'self' blob:
in production CSP; set restricted devCsp compatible with Vite/HMR rather than
leaving dev unrestricted. No unnecessary eval, remote scripts or broad capability.
Use stable model URIs keyed by session/endpoint/path; dispose editors/models/workers
and listeners on close. Match existing appearance/code font and pane resize.
Provide side/inline modes, hide unchanged, whitespace option, overview ruler,
selected-hunk details, prev/next and dirty/save state. Either WT endpoint can edit;
commits cannot. For editable-left honor immutable/right orientation by a tested
adapter (Monaco originalEditable or deliberately mapped model roles), retaining
labels, signed counts and copy directions. WT/WT changes remain independent.
Save invokes P5, tracks expected disk bytes and preserves encoding/EOL. Model undo
is distinct from filesystem undo; successful save alone clears disk-dirty state.
Refresh/close/root change with unsaved edits offers save/discard-buffer/cancel,
not silently overwrite. Binary, oversized, undecodable, type-conflict and opaque
gitlink views have an explicit noneditable fallback. Zero/pending/timeout diff
computations must never enable stale hunk actions.
Check A6: real WebView2 scene 5, both WT orientations, inline/side, collapse,
whitespace, overview/nav, theme/font/resize, save/undo/reopen and unsaved guards.
Inspect worker loading and CSP violations; verify emitted local worker assets in
release output once approved, since dev success does not prove production CSP.
Failure: network worker, blank editor, leaks, swapped write destination, altered
EOL/encoding or save claims success on conflict.

### P7 [ISOLATE] Directional Copies And Shortcuts (R7, I2, I3, I5)

Depends on P6. Targets: `files.rs`, `paths.rs`, `FileCompare.svelte`,
`FolderCompare.svelte`, `compare.svelte.ts`, `commands.ts`, `CommandPalette.svelte`.
Use one command registry for icon/context-menu/palette/key actions and eligibility.
Copy left-to-right/right-to-left only when destination is WT. Hunk arrows apply
Monaco's current valid line mapping to the destination BUFFER, preserve EOL and
participate in model undo; Save performs recoverable disk mutation. Disable hunk
copy for binary/large/type-conflict/pending diff and immutable targets; orphan text
may have a full insertion hunk after a valid model diff, never synthesized indices.
Whole-file copy preserves source bytes through P5. Missing-source direction is
disabled (not deletion); source-only creates destination only through safe parents.
Directory copy walks a bounded validated snapshot, merges source-present files,
shows overwrite/create preview, confirms replacements and retains destination-only
files. No recursive symlink/submodule following, implicit rename or mirror-delete.
Dirty destination buffers require save/discard-buffer/cancel before disk copies.
Undo batch uses P5 journals, checks later edits and surfaces partial restore.
Propose F7 next / Shift+F7 previous, Ctrl+Alt+Right copy L->R,
Ctrl+Alt+Left copy R->L, Ctrl+K palette; Ctrl+S save in editor. Ctrl+Alt+arrows
avoid standard Monaco Ctrl+arrows/Shift+Alt+arrows bindings, but may collide with
Windows/driver hotkeys, so retain clickable equivalents and approve exact keys.
Keys are scoped to active compare/focus and modal state, absent from normal text
inputs; do not hijack global Ctrl+Z or prototype scene-switch arrow shortcuts.
Check A7: each direction through icons, menus, palette and approved keys; read-only
disabled, orphan insertion, folder retention, preview/cancel, undo and external-edit
conflict. Assert source/destination bytes, not just success toasts.
Failure: opposite endpoint modified, implicit deletion, stale hunk or shortcut theft.

### P8 [BATCH] Whole-Set Compare And Final Acceptance (R8, I1, I4, I5)

Depends on P7. Targets: `compare.rs`, `lib.rs`, `api.ts`, `compare.svelte.ts`,
`SetView.svelte`, `commands.ts`; new `src/components/SetCompare.svelte`;
`README.md` and `CHANGELOG.md` for approved delivered behavior only.
Freeze selected set ID and EVERY item ID (not only checked clone rows); keep
duplicate repository folders separate. Resolve both common ref selections per
repo using P3; do not collapse failures into an overall error. Share fetch dedupe,
limit parallelism/cancel, and publish progressive per-item results. Show missing
left/right/both, not-cloned/unavailable, actual failure, identical and differences
separately. Aggregate only completed comparable rows; show completion denominator,
unknown binary line counts, N/A ancestry and exclusion/normalization policy.
Differences-only retains unavailable/missing rows. Click comparable summary rows
(including identical) to open scene 4 with exact endpoints/session options; missing
rows show their reason without pretending to open a valid compare. Refresh or set
changes cannot reuse stale summaries; session is not persisted in settings.
Check A8: live scene 6 fixture set containing identical/changed/orphan/binary,
missing refs on each side, duplicate folders and uncloned rows. Counts equal
corresponding scene-4 results, summary clicks preserve context and no auto-clone.
Failure: one repo cancels the set, wrong duplicate target, failed row counted as
identical, fake shortstat totals or scope creep into Phase 2.

## Verification And Review Gates

P1/P2/P3 checks and disposable desktop dev are approved; P4-P8 remain future gated work.
Frontend execution/package management uses latest stable Bun, verified against the
official `oven-sh/bun` release endpoint. Current verified stable: 1.4.2 (2026-10-01).
Use `rtk bun install --frozen-lockfile` once the conservative import is verified;
retain the existing npm lockfile as migration evidence, do not update dependencies.
Every P part: from repo root `rtk bun run --bun check`; from src-tauri
`rtk cargo clippy --all-targets` with no new warnings compared with recorded baseline.
Known existing warnings must be recorded, not fixed through unrelated cleanup.
Rust behavior parts also run `rtk cargo test --lib <module_name>::tests`, using
inline #[cfg(test)] modules in git/compare/paths/files. Use std temporary fixtures
and existing tokio/serde; add no test framework just for this change. P1 migration
fixtures use a pure extracted workspace module and Bun's built-in runner
(`rtk bun test src/lib/workspace.test.js`), without a new test framework.
No existing frontend test harness: component keyboard/layout/editor interactions
are manual in the actual desktop WebView, not claimed covered by svelte-check.

Worker runs focused A1-A8 for the touched part, plus a regression smoke of all I1
flows using disposable roots and nonproduction sample repos. Start actual desktop
with `rtk bun run --bun tauri dev`; Vite port 1420 must be free or an explicitly
adjusted matching dev URL used. Capture prototype-vs-app screenshots at native
1440x900 and minimum window size, light and dark, plus keyboard/Activity evidence.
The fixed Windows desktop minimum, not a new mobile redesign, controls layout.
P6 additionally uses separately approved `rtk bun run --bun build` and release Tauri execution (an
approved `rtk bun run --bun tauri build --no-bundle` followed by launching its binary)
to test production CSP/worker assets. Build is necessary evidence later, forbidden
now. Do not imply a dev-only smoke verifies packaged CSP.

For EACH A<n>: proof owner = worker (fixture/static/live evidence) plus independent
reviewer (read-only code review against this brief/prototype, replay or witnessed
live evidence); user reviews the small part and authorizes proceeding. Reviewer
checks invariant coverage and failure/recovery evidence, not screenshots alone.
No next part on unsigned acceptance, new warnings, failed recovery or unexplained
diff. Reviewer must not write into the worker's worktree.

## Risks, Recovery And Topology

- High / P5-P7: Windows races/replace/locks, disk loss and partial batch recovery.
  Mitigation: handle-based path proof, verified durable backups, fail closed,
  crash/restore drills before exposure. No mutation without user authority.
- High / P2: credentials and source contents in output; redaction before every
  sink and chunk-boundary tests; safe blob output representation and bounded memory.
- Medium / P3/P8: cross-repo semantics, shallow history and normalization drift;
  inventories plus honest N/A, raw/display distinction and shared engine fixtures.
- Medium / P1: saved-state loss and active-set bleed; additive versioned migration,
  ID-based tabs, round-trip legacy settings evidence.
- Medium / P6: Monaco model orientation and Vite/WebView2 CSP; test both editable
  sides, disposal and release workers; restrict language payload to actual needs.
- Medium / P5: backups retain private source and can fill app-data drive; disclose
  location/retention, enforce capacity with fail-closed writes, no silent pruning.

After plan approval ONLY: create one new local branch with a user-approved or
routine descriptive name; no push. Single worker/writer, staged P1-P8 handoffs,
reviewer read-only. Do not create extra branches/worktrees or commits without
separate permission. Preserve unrelated changes and untracked prototype.
Rollback application changes via reviewed inverse changes, never reset user work.
Recovery owner = user, assisted by worker; journal restore validates recorded
path/root identity and checksums, then reopens content and refreshes compare/status.
Point of no return = explicitly approved deletion of a recovery backup; no such
cleanup is automatic. For failed reclone retain both original backup and partial
clone, offer safe recovery without overwriting either.

## Approval Decisions And Stop Conditions

- D1 / user / approved: F7/Shift+F7, Ctrl+Alt+Right/Left, Ctrl+K and Ctrl+S as
  proposed in P7; only Ctrl+K is implemented in P1. Retain buttons
  if OS intercepts them. Alternatives: custom bindings, not conflicting editor keys.
- D2 / user / approved: P5 persisted private backups plus bounded session undo
  (recommended) versus .bak-only or session-only. Session-only cannot recover after
  restart; .bak-only pollutes repos and lacks conflict-safe operation history.
- D3 / user / approved order (execution currently through P3): P1 shell -> P2 Activity/tree -> P3 backend -> P4 UI
  -> P5 safe writes -> P6 Monaco -> P7 copy/keys -> P8 set compare. This splits the
  suggested shell and copy stages to isolate shared execution and recovery risk.
- Scope/prototype/stack/Heavy/staged review ownership are settled, not reopened.
  Monaco is approved for P6 only, not installed in P1. The exact Windows API crate,
  version and API must be shown for approval before installation in its future part.

Stop and return a located revision request on unsupported Windows containment,
unverified backup, failed restart restore, writer collision, missing authority,
unexplained saved-state drift, production worker/CSP failure, dependency scope
expansion or a requirement that needs Phase 2. Do not silently weaken safety or
invent approval. Current gate: independent P3 review, then user acceptance.
Next owner: read-only reviewer. P4 is not authorized by passing P3 worker checks.

## Revision Log

- 2026-10-01: Initial evidence-backed proposal; no product implementation or builds.
- 2026-10-01: User approved order, shortcuts, persisted private recovery plus session
  undo, future P6 Monaco and P1-only branch/execution; requires latest stable Bun.
  P2-P8 remain gated on independent review and per-part user acceptance.
- 2026-10-01: User explicitly approved moving to and recovering P2. Existing
  implementation and original live results retained. Fresh runner/tree fixtures,
  Bun tests/check and all-targets Clippy pass without new warnings; focused live
  IPC cancellation/Activity recovery passes. Only the harness, live recovery
  evidence, report and packet were changed during recovery. One harness assertion
  was corrected for legitimate concurrent refresh jobs; no product defect was
  confirmed or repaired. Independent review/user P2 acceptance remain pending.
- 2026-10-01: Final independent P2 technical approval and user P1/P2 checkpoint
  precede the explicit P3-only request. Implemented the read-only engine, path
  boundary, structured IPC and minimal frontend state; required gates and real
  disposable IPC checks pass. Updated only this packet and `evidence/S3-report.md`
  for the handoff. Protected Paperwing designs and installed assets unchanged.
  No further commit, dependency, image, rename/palette or P4 work; P3 review pending.