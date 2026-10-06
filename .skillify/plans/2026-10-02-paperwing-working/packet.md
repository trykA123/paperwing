# PaperWing working plan — Linux parity, maintainable boundaries, measured performance

**Status:** Proposed; planning only. No application implementation, dependency install, GitHub mutation, commit, push, release, or version bump is authorized by this document.
**Weight:** Heavy umbrella; child packets specify their own weight.
**Decision owner:** User. Main session integrates approved work. One application-code writer per checkout; separate worktrees for concurrent writers.
**Read first:** `README.md`, `docs/improvements.md`, this plan, [engineering rules](rules.md), then the selected child packet and its dependencies.

## Intent and outcome

PaperWing keeps Tauri 2/Rust, Svelte 5/TypeScript, Vite and Bun. Deliver the same user-facing functionality on native Linux and Windows, smaller responsibility-based modules/components, and measured improvements to comparison responsiveness. Linux viewing-only builds are intermediate evidence, not feature-complete acceptance. Prove the workflows with local adversarial fixtures and disposable repositories in the user's own GitHub account.

Priority: data protection and correct final results > Windows/Linux functional parity > maintainable ownership > cold foreground responsiveness > warm reuse > optional speculation.

## Scope

- In: workspace/source/settings persistence; GitHub credentials and repository discovery; manual/GitHub Enterprise contracts; clone/open/fetch/pull/switch/reclone; duplicate checkouts; refs/local trees/status/Activity/cancellation; all current comparison modes/options/whole-set drilldown; Monaco viewing/editing/hunk and whole-file directional copy; staging/commit/separate explicit push; branches; folder recycling; persisted undo/recovery; native artifacts and documentation.
- In: characterization, responsibility-based extractions, metadata correctness, foreground optimization, progressive delivery, bounded immutable RAM reuse. Conservative local prewarming is conditional.
- Out: framework replacement, a database/server, visual redesign, new global search, heat/LFU/ARC machinery, persistent source/blob/diff caching, speculative fetch/checkout/push, a GitHub repository-management product feature, macOS support expansion, release publication and changes to any assistant configuration.
- Exclude `docs/paperwing-tour.svg` from analysis and execution; it is an unrelated animation. Preserve the user's already-untracked `docs/improvements.md` unchanged.

## Decisions, facts, and open gates

- [DECISION — user] Linux must have Windows feature parity, including save/copy/undo/recovery. A read-only final product is rejected.
- [DECISION — user] Future acceptance may create disposable repositories in the user's own GitHub account. Exact account, names, visibility, credential mechanism, remote actions and cleanup must be recorded before use. This planning phase performs no remote operation.
- [DECISION] Preserve Windows NTFS/TxF safeguards; do not obtain Linux compilation by removing guards. Implement a separately proven native Linux backend.
- [DECISION] Refactor behavior separately from intentional changes. Keep stable façades and ownership; no arbitrary file-length ceiling or generic `utils`/controller dumping ground.
- [DECISION] Foreground first. Neither cold optimization nor progressive delivery depends on caches or prewarming.
- [FACT] `files.rs` and `file_guard.rs` and write-service registration in `lib.rs` are Windows-only. `FileCompare.svelte` asks for edit tickets before constructing a working-tree editor. Linux support is not established by the frontend being cross-platform.
- [FACT] `Cargo.toml` enables Windows/Apple keyring features, not an explicit Linux credential backend. `tauri.conf.json` bundles only NSIS. Workspace defaults and several path identities are Windows-oriented.
- [FACT] `compare.rs` is 3,961 lines, of which its final roughly 1,422 lines are embedded tests. `state.svelte.ts` is 689 lines, `git.rs` 1,051 and `app.css` 5,486. Size alone does not identify a responsibility boundary.
- [FACT] The Bun test loader matches only `state`/`compare` rune module endings. Rust's layout-golden test reads a declaration from `workspace.test.js`. Both are refactoring hazards.
- [FACT] Preparation eagerly reads/classifies content, calculates counts and history before publishing; the frontend then collects every page before displaying the list. Different blob IDs do not imply different normalized display content.
- [FACT] On 2026-10-02 the planning host reported Linux 7.2.4-3-cachyos x86_64, Bun 1.4.2, Rust/Cargo 1.98.1, Git 2.55.0, GTK 3.24.52 and WebKitGTK 2.52.6. These are tool/preflight observations, not a successful app build or launch.
- [ASSUMPTION] The current Linux host is a useful first reference environment. Check in packet 01; if the user's home host differs, add its native evidence rather than generalizing this host's results.
- [OPEN — packet 08] Linux's supported filesystem/kernel envelope, native replacement/removal mechanism, concurrency guarantees, durable recovery and metadata policy require design/prototype evidence and user acceptance. Ordinary rename plus advisory locks is not assumed equivalent to TxF/share-mode exclusion.
- [OPEN — packet 18] Ratify provisional/final comparison statuses, filtering and enrichment before changing IPC. Exact normalized folder equality can require reading unopened files; strict lazy-only-on-open cannot be promised simultaneously without a changed contract.
- [OPEN — packet 01] Ratify fixture/hardware-specific performance targets after measuring. The old document's p95 figures and ~128 MiB are hypotheses, not measured acceptance promises.

## Requirements and invariants

- R1: The native Windows/Linux feature matrix below passes in supported environments; unsupported environments fail explicitly rather than losing data.
- R2: Large source files have cohesive named owners and smaller leaves while compatibility façades preserve callers and contracts.
- R3: Metadata is scope-correct, retryable, deduplicated and invalidated for branch/source/credential/ref/root changes.
- R4: Cache-empty first useful rendered results and full completion improve in measured fixtures; pending output is honest and cancellable.
- R5: Immutable reuse and optional background work obey known byte/work/lifetime bounds and never grant authority.
- R6: Reproducible local and disposable-GitHub evidence and fresh native artifacts substantiate parity; no release/history overwrite.
- I1: Preserve containment, metadata protection, fresh session/generation checks, expected-byte/root/metadata validation, dirty-buffer guards, durable backups and conflict-safe recovery.
- I2: Cached inventories/bytes/rows never contain `Prepared`, write tickets, filesystem leases, jobs or editor models and never authorize a mutation.
- I3: Final results preserve normalization/EOL/whitespace, binary/unavailable/type/mode/link/submodule/history/rename behavior and limits. Destination-only files remain retained by directional copy.
- I4: Copy batches and multi-repository Git actions remain partial, not fictitious all-or-nothing transactions. Commit still commits the complete staged index, not merely the displayed list.
- I5: No speculative network fetch or mutation. Explicit push stays separate. Credentials and source/file contents never enter benchmark logs, plans, committed fixtures or cache keys.
- I6: Preserve saved settings, existing Windows recovery records, user repositories, unrelated edits and older plans. Unavailable native checks are recorded as skipped, not passed.

## Engineering rules

[rules.md](rules.md) applies to every implementation and review under this plan. Include it in each child-packet handoff alongside the selected packet and dependency evidence. Check applicable rules before accepting a slice; record justified exceptions in that slice's evidence. Numeric thresholds prompt review and never override cohesive ownership, safety invariants, compatibility contracts or the selected packet's approved scope.

## Packet graph and handoff order

Each link is a separate packet. Dependencies are **completed acceptance**, not merely existence of a document. Approve one packet or explicitly enumerated group; do not infer authority for later packets. A gated implementation packet is not execution-ready until its named design is accepted.

| ID | Packet | Depends on | Gate / role |
|---|---|---|---|
| 01 | [Baseline, fixtures and characterization](../2026-10-02-paperwing-01-baseline/packet.md) | none | First execution candidate; establishes evidence, R6/I6 |
| 02 | [Comparison responsibility boundaries](../2026-10-02-paperwing-02-compare-boundaries/packet.md) | 01 | Behavior-preserving extraction; R2/I1-I3 |
| 03 | [Frontend state responsibility boundaries](../2026-10-02-paperwing-03-state-boundaries/packet.md) | 01 | Behavior-preserving extraction; R2/I1/I6 |
| 04 | [Presentation component boundaries](../2026-10-02-paperwing-04-ui-boundaries/packet.md) | 01,03 | Behavior-preserving extraction; R2/I1 |
| 05 | [Ordered CSS chapters](../2026-10-02-paperwing-05-css-boundaries/packet.md) | 01 | Behavior-preserving extraction; R2/I6 |
| 06 | [Windows file-write boundaries](../2026-10-02-paperwing-06-windows-write-boundaries/packet.md) | 01 | Mandatory native Windows proof; R2/I1/I6 |
| 07 | [Git runner/query boundaries](../2026-10-02-paperwing-07-git-boundaries/packet.md) | 01 | Behavior-preserving extraction; R2/I5 |
| 08 | [Linux write safety design and proof](../2026-10-02-paperwing-08-linux-write-contract/packet.md) | 01 | Design/prototype only; user ratifies native contract; R1/I1 |
| 09 | [Native roots, paths, identity and capabilities](../2026-10-02-paperwing-09-platform-paths/packet.md) | 02,03,08 | No Linux write enablement; R1/I1/I6 |
| 10 | [Linux secure credentials](../2026-10-02-paperwing-10-linux-credentials/packet.md) | 04,09 | Native locked/unavailable/restart proof; R1/I5 |
| 11 | [Linux Git process lifecycle](../2026-10-02-paperwing-11-linux-git-lifecycle/packet.md) | 07,09 | Descendant cancellation/timeout proof; R1/I4 |
| 12 | [Linux protected filesystem primitives](../2026-10-02-paperwing-12-linux-file-guards/packet.md) | 06,08,09 | BLOCKED until accepted packet-08 contract; R1/I1 |
| 13 | [Linux durable journal and recovery](../2026-10-02-paperwing-13-linux-recovery/packet.md) | 12 | Crash/restart/undo proof; R1/I1/I6 |
| 14 | [Linux editor/save/copy integration](../2026-10-02-paperwing-14-linux-write-workflows/packet.md) | 02,04,09,13 | Native tickets/dirty/copy workflows; R1/I1/I4 |
| 15 | [Linux clone/reclone and desktop trash](../2026-10-02-paperwing-15-linux-folder-workflows/packet.md) | 09,11,12 | Separately confirmed filesystem mutations; R1/I1/I4 |
| 16 | [Metadata correctness, personal discovery and single-flight](../2026-10-02-paperwing-16-metadata-correctness/packet.md) | 03,09,10 | Authenticated private-owner discovery and all cache readers; R1/R3/I5 |
| 17 | [Measured cold-path efficiency](../2026-10-02-paperwing-17-cold-path/packet.md) | 01,02,09 | Select only measured bottlenecks; R4/I3 |
| 18 | [Progressive comparison contract](../2026-10-02-paperwing-18-progressive-contract/packet.md) | 01,02,17 | Design only; user accepts provisional semantics; R4/I1/I3 |
| 19 | [Progressive backend producer](../2026-10-02-paperwing-19-progressive-backend/packet.md) | 09,18 | BLOCKED until packet-18 contract accepted; owns interactive/enrichment priority classes; R4/I1/I3 |
| 20 | [Progressive frontend consumer](../2026-10-02-paperwing-20-progressive-frontend/packet.md) | 03,04,19 | Actual first useful render; R4/I1/I3 |
| 21 | [Bounded immutable RAM reuse](../2026-10-02-paperwing-21-immutable-cache/packet.md) | 21a: 12,16,17,20 · 21b: 21a + native write proof per platform (Windows existing path; Linux 14) | Read hits first, write-after-hit later; owns app-wide memory budget for 17–22; R5/I1-I3 |
| 22 | [Conservative foreground-prioritized prewarm](../2026-10-02-paperwing-22-prewarm/packet.md) | 11,16,19,20,21a | OPTIONAL: adds speculative class to 19's scheduler; needs measured Windows benefit/approval; R5/I5 |
| 23 | [Native parity and own-account GitHub acceptance](../2026-10-02-paperwing-23-native-github-acceptance/packet.md) | 02-07,10,11,14-17,20,21a (21b where enabled); 22 if enabled | Full feature matrix, Windows first then Linux; R1-R6/I1-I6 |
| 24 | [Fresh native artifacts and support docs](../2026-10-02-paperwing-24-native-artifacts/packet.md) | 23 | Clean-profile standalone acceptance; R1/R6/I6 |

Safe read-only discovery can run in parallel. Packets 02/03/05/06/07/08 share a baseline but are not permission for multiple writers in one checkout. Integrate one green slice at a time. Prioritize 08 early: do not invest in a Linux write implementation whose protection contract is unresolved. Mandatory refactor/performance work can continue while that decision is investigated.

## Responsibility map (proposed destinations)

- `compare.rs` remains the Tauri façade/session-authority coordinator. `compare/{tests,registration,inventory,text_diff,history}.rs` own tests, saved endpoint binding, inventory/content processing, normalization/counts and history. Never describe handles-bearing inventory as a globally cacheable pure value.
- `state.svelte.ts` remains app/tab/dirty-buffer/persistence coordination. `state/{repository-metadata,repository-trees,git-activity}.svelte.ts` own their state and invalidation; `workspace-paths.ts` owns pure destination calculations; `set-compare.svelte.ts` owns bounded whole-set coordination.
- `files.rs` remains the IPC façade. `files/{journal,tickets,copy}.rs` own journal, authorization lifetimes and frozen plans; `file_guard/{windows,linux}.rs` or an accepted equivalent owns OS primitives. Packet 08 settles shared versus backend-specific journal boundaries; do not force incompatible native operations into a misleading transaction trait.
- `git.rs` remains compatibility/command façade. `git/{runner,redaction,validation,repository_tree,remote_refs,tests}.rs` have one owner each; runner/activity/cancellation sharing remains together.
- Presentation children receive explicit data/callbacks. FileCompare retains Monaco/ticket/dirty ownership; Settings retains drafts; SetView retains virtual-row/page state. CSS is split in original flattened cascade order, not regrouped by selector.

## Native feature acceptance matrix

Packet 23 records PASS/FAIL/SKIPPED and evidence for **both platforms** for every row. A skipped required Linux or Windows row prevents a parity-complete claim.

| Workflow | Required observation |
|---|---|
| Sources and persistence | GitHub personal account token storage/restart/delete; locked-store errors; manual URLs; Enterprise URL validation/contract fixtures; settings/theme/font/set/ref/destination restart |
| Workspace/clone | New set/add/duplicate/custom layout; clone/open/fetch/pull/switch/reclone; parallel/cancel/failure behavior; unsafe/shared roots protected |
| Repository/Git | Branches/tags/HEAD/stashes/submodules/status/Activity; bounded/redacted output; stage/unstage/full-index commit; create/switch/delete local branch; explicit publish/push |
| Comparison | working↔working, HEAD↔working, immutable↔immutable and cross-checkout; options/exclusions; Same/Differences; rename/mode/link/binary/unavailable; whole set and drilldown; no checkout for snapshot comparison |
| Editor/copy | Monaco first load, selection/focus/shortcuts, EOL/BOM roundtrip, dirty-close/mutation guards, hunk copy, whole-file/folder preview/cancel/confirm, creates/overwrites and destination-only retention |
| Recovery | Save then undo; copy batch partial outcomes and reverse undo; restart; conflict acknowledgement; cleanup only eligible backups; stale ticket/source/root/metadata refusal; interrupted write stages |
| Folder removal | Confirmed local set-folder recycle into desktop trash, preserved shared/unregistered folders, honest partial failures and preservation of recoverable checkout data |
| Native shell/artifacts | Dialogs, clipboard/keyboard, opener/VS Code if installed, light/dark/focus/disabled states; native production WebView, standalone executable and platform bundle launch |

Personal GitHub evidence does not establish Enterprise server interoperability: prove host parsing/contracts with controlled fixtures and mark live Enterprise checks not run. The current UTF-8/path/content safety envelope and unsupported file kinds remain explicit; feature parity does not imply arbitrary filesystem/name support.

## Common execution and verification rules

1. Re-read [rules.md](rules.md), the selected packet and exact dependency evidence. Confirm `git status --short` and preserve unrelated changes. All new module/script/evidence destinations are proposals until created by an approved implementation packet.
2. Record toolchain and test discovery before the first source move. Frontend server-render tests do not prove client effects, WebView lifetimes or native filesystem safety. Linux passing tests cannot prove Windows-only behavior.
3. Use plain commands, not an assumed `rtk` wrapper. Packet 01 explicitly removes the existing Windows junction-test wrapper dependency while preserving its assertions. If dependencies are absent, obtain approval for `bun install --frozen-lockfile`/Cargo downloads; do not upgrade lockfiles to make a baseline green. Capture exit codes and ignored/filtered test counts.
4. Quality gates from repository root:

```sh
bun run --bun check
bun test src/lib/workspace.test.js src/lib/workspace.test.ts
bun run --bun build
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git -c core.whitespace=cr-at-eol diff --check
```

New test suites are added explicitly to packet commands (or tested via `bun test src/lib` once characterized). Rust timing/single-global-runner tests run serially. Stop on newly missing test discovery or unexplained baseline failures; do not hide a failed native property behind mocks.

5. Native release build/launch: `bun run --bun tauri build --no-bundle`, then execute the new artifact in a disposable profile with isolated app data/cache/config/WebView storage. Packet 01 supplies a test-only Tauri identifier and verifies actual resolved storage; an ignored binary folder alone is not isolation. Keyring service/source IDs require independent isolation from normal `paperwing`/legacy `flock` credentials. Never connect a test profile to real checkout roots.
6. Benchmarks: deterministic local fixtures plus separate opt-in GitHub latency. Record hardware/revision/build flags, data/result fingerprints, cache state, queue/Git/CPU/IPC/first useful render/completion timings, per-operation command counters independent of bounded Activity, p50/p95 and variance, retained/in-flight/process-tree memory. Use an opt-in instrumented **release** build; debug unit tests are not latency proof. No OS-cold-cache claim from restarting the app.
7. Stage every extraction green before a behavior change. Independent reviewer checks applicable engineering rules, the diff and supplied checks; read-only reviewers cannot run tests. Record justified rule exceptions in the slice's evidence. Do not commit/push automatically.

## Heavy gates, rollback, and authority

- User owns destructive/native protection contract changes, credential backend selection, data/record compatibility, remote fixture actions and accepted risk. Main session integrates; the worker receives one approved bounded packet.
- Dedicated branch/worktree for implementation and disposable local roots/profiles. Record a backup manifest and prove restoration on a sacrificial fixture before the first write/recovery/folder-operation drill. Do not create that branch or run a drill during this planning phase.
- Preserve existing Windows record serialization and restore support. A Linux schema/namespace is accepted in packet 08/13; never reinterpret Windows volume/index fields as Linux identity or auto-delete old records.
- Roll back an extraction by restoring only its owned source changes; roll back performance through bypassing/clearing disposable cache entries, not repositories/settings/recovery.
- Roll back native-write rollout by disabling new writes while keeping a **forward-compatible recovery reader/export/undo path** for newly created records. Restore fixture bytes from verified backups after inspection. Downgrading the executable alone is not a recovery plan.
- External push/publication, repository deletion and eligible-backup cleanup are points of no return from the app's perspective; require action-specific approval and external/local recovery evidence. No force-push test is required or authorized by default.
- Stop on missing authority/design evidence, failed restore drill, unexpected drift, an unsafe race, untested filesystem assumptions, secret exposure, changed comparison semantics, unbounded work, concurrent-writer collision or unavailable required native proof. Return to the user; do not silently reduce Linux's final functionality or Windows protection.

## Assessment of the earlier performance packet

Keep it and `docs/improvements.md` as references, not execution authority. Its immutable-key/fresh-session distinction, bounded ownership and safety invariants are useful. It does not cover full Linux functionality or systematic modularity, places speculative prefetch before foreground improvements, underspecifies normalized provisional results and makes unmeasured policy/targets too prescriptive. This graph corrects those issues without overwriting the historical proposals.

## Overall acceptance

- A1 (static + fixtures): packets 02-07 demonstrate cohesive boundaries and identical characterized contracts → R2/I1-I6.
- A2 (native fixtures + owner-observed): accepted Linux safety design, native guard/journal/ticket/folder tests and feature matrix pass on both platforms → R1/I1/I4/I6.
- A3 (fixtures): branch/source/credential/root changes, concurrent requests, retry, force refresh and late results are correct → R3/I5.
- A4 (native instrumented release): cache-empty first useful rendered results improve on ratified fixtures; progressive and cold final output agree → R4/I1/I3.
- A5 (fixtures + native measurements): cache/prefetch bounds, pruning/root replacement/external-write/active-editor cases pass; optional 22 is omitted if unproven → R5/I1-I5.
- A6 (live + owner-observed): own-account disposable GitHub matrix and fresh Linux/Windows artifacts pass; cleanup manifest shows no unrelated mutations → R6/I6.

## Planning verification (not implementation acceptance)

- 2026-10-02: Document checks passed for 24 child packets plus this index: links exist, paths are not ignored, step/requirement acceptance references are consistent, dependency headers match the table, and the graph is acyclic including optional 22.
- The user's improvements note and all three older packet SHA-256 values remain unchanged. Git reports no tracked product changes; the only new repository files are these planning documents.
- Independent read-only scout reconnaissance used the configured Luna role. Sol 6.1/xhigh reviewed the full initial plan, located four material gaps, then approved those four planning repairs after rechecking. This is not approval to implement or proof of native functionality.
- App tests/builds/benchmarks, credential persistence, Linux write algorithms, native Windows/Linux workflows, official API contracts and live GitHub operations were not executed or proven during planning. All remain explicit execution gates.

## Revision log

- 2026-10-02: New proposed working plan and child packets. Full Linux functional parity is the user's goal. No app tests/builds/benchmarks, native backend design approval or GitHub operation claimed.
- [REV 2026-10-02] Independent review: 21 now requires enabled Linux write workflows (14); 16 explicitly owns authenticated private personal-repo discovery; 01 owns the Windows test-only wrapper correction; 23 pins immutable remote fixture IDs and refuses unbound automatic cleanup.
- [REV 2026-10-02] Added shared Rust/Svelte engineering rules to required reading, child-packet handoffs, implementation acceptance and review. Numeric size thresholds remain review triggers.
- [REV 2026-10-02] Extended engineering rules with structured failures, Rust/TypeScript IPC consistency, testable calculations and bounded large-inventory rendering.

- [REV 2026-10-03] Authorized execution of01–09 completed on Linux for01–05/07–09;06deferred without changes because mandatory nativeWindows proof is unavailable. Owner explicitly accepted08practicalrace limits. Independent review approved09read-only platform/path/capability boundary. Remaining Linux functionality requires later packets; no full parity or release claim. Current status: docs/implementation-status.md.

- [REV 2026-10-03] Owner approved continued implementation of the remaining plan and improvements found during execution. Concrete10backend/dependency/privatewallet proposal is approved. Existing unavailable nativeWindows gates stay explicit; this does not claim full parity or authorize changing unrelated data.

- [REV 2026-10-04] Credential entropy/runtime repairs independently approved and integrated; combined native revalidation underway. Packet12 repair source and native responsiveness evidence are frozen. Reviewer usage limit interrupted its recheck, so12acceptance and13implementation handoff remain pending. Linux writes remain disabled;13concrete design is prepared without product changes.

- [REV 2026-10-04] Combined10–12 verification passes80/85 Rust suites,63 frontend tests, production builds and fresh native KSecrets n5. Native comparisons preserve baseline output/counters.12repair review remains pending;13handoff is held at the dependency gate. Earlier09preview crashed with heap corruption and no available core; current production smoke evidence is separate and no root-cause fix is claimed.

- [REV 2026-10-04] Packet12 repair recheck approved;13durable Linux recovery implementation proceeds under the owner’s existing authorization. New save/copy rollout remains14-owned and requires13native recovery acceptance.

- [REV 2026-10-04] Packet13 original source/evidence frozen at a026a7938a2838d8562365c7366d307db76a15f1184b1b1a663a12a7cbd2e5ee. Independent review requested two implementation repairs: fresh lock authority before resumed unlink and reliable owned test-child failure cleanup. Repair-1 has isolated native controls and a separate immutable handoff.14planning proceeds;13integration and application write rollout await repair approval.

- [REV2026-10-04] Packet13 library accepted after independent repair approval and18-path integration. Combined118/123 Rust and63 frontend tests pass; the normal Linux production build and owned empty-profile smoke pass. Linux write commands remain disabled. Packet14 proceeds through individually reviewed storage/parent/ticket/copy/UI slices; user approval persists.

- [REV 2026-10-05] Owner: PaperWing is Windows-focused (work use). Packets 15–24 rewritten in plain format with Windows-first native proof. 17 commits to cutting per-file Git processes/temp files (numstat batch, persistent cat-file, optional gix). 19 now owns foreground/enrichment priority (moved from 22). 21 split into 21a (read hits, no longer gated on 14) and 21b (write-after-hit, per-platform native proof) and defines the app-wide memory budget for 17–22. Packets 01–14 untouched (in progress).
