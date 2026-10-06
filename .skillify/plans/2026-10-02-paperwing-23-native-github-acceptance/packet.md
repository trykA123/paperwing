# Packet 23 — Full native acceptance with disposable GitHub test repos

| | |
|---|---|
| Status | Proposed. Needs approval of the exact test account, manifest and actions. |
| Weight | Heavy (live remote and filesystem data). |
| Depends on | Accepted 02–07, 10, 11, 14–17, 20, 21a (21b where write-after-hit is enabled); 22 only if enabled. Transitively includes 08, 09, 12, 13, 18, 19. |
| Primary platform | **Windows first** (work use), then Linux. Windows results gate acceptance. |
| Read first | Working plan's native feature matrix and Heavy rules; all dependency evidence; packet 01 runner; accepted Linux and progressive contracts. |

## Goal
Prove every feature end to end on native Windows and Linux, using repeatable local adversarial fixtures and disposable repositories in your own GitHub account. This tests existing workflows; it does not add repository creation or deletion to PaperWing. Existing real repositories, organization repositories and releases are never touched.

## Scope
- **In:** the full feature matrix on both platforms; live own-account discovery, clone, branch, commit and explicit push; fault, cache, progressive and performance replays.
- **Out:** new features, Enterprise live testing (fixture-only, labelled as such), release publication.

## Requirements
- **R1** — Every required feature row runs in the native production WebView on Windows and Linux and passes where supported.
- **R2** — Own-account fixtures cover private-token persistence, repository discovery, remote refs, clone, branch, commit, explicit push and meaningful compare, copy and recovery.
- **R3** — Failure, interruption, external-change, cache, progressive and restart cases prove the accepted safety rules and exact final output. Performance targets can be replayed.
- **R4** — Account, visibility, names, actions, cleanup and backup evidence are explicit. Remote authority is pinned to immutable owner and repository IDs. No unrelated data or credentials are exposed.
- **I1** — Remote changes stay deliberate: no force push, no token-scope expansion, no default repository deletion, no speculative network. Push stays a separate user action after commit.
- **I2** — Every file or folder change uses the accepted protections, a verified fixture restore and native proof. No skipped Windows or Linux row is called "parity complete".

## Decisions
- **D1 (owner)** — Disposable repositories may be created in your own GitHub account for these tests. Planning performs no such operation.
- **D2 (proposed)** — Private repositories named `paperwing-test-<run-id>-a` and `-b`, kept until acceptance and cleanup approval. Use a marker manifest, an isolated native profile, root and keyring namespace, and no checked-in credentials.
- **D3** — Personal GitHub testing cannot prove live Enterprise compatibility; test Enterprise host, auth and request contracts with fixtures and label live Enterprise evidence as missing.
- **D4** — Run every matrix on native Windows first, with real-time antivirus on, because that is the work environment. Linux follows.

## Assumptions
- The account can create private repositories and authenticate Git separately from the GitHub API. Step 1 verifies this without printing tokens. If not, ask for a scoped alternative; never fall back to public or third-party repositories.

## Steps
1. **Confirm the plan:** exact signed-in owner, visibility, run ID, fixture names, local roots and profiles, least-privilege credential mechanism, and the explicit create/seed/push/optional-delete actions. Produce a dry-run manifest and a sacrificial local restore.
   - Where: proposed `scripts/testing/github-fixtures.ts` and an ignored acceptance manifest.
   - Check: `bun run scripts/testing/github-fixtures.ts --dry-run --owner <approved-owner> --run-id <approved-id>`; the owner accepts the exact actions. The account is never inferred from a repository origin or an arbitrary existing `gh` login.
2. **Create and seed only the approved repositories** through a reviewed API/CLI path or owner-observed GitHub UI. Use deterministic branches, tags, history, divergence, renames, EOL, whitespace, binary, large text, type, mode, link, submodule and destination-only fixtures, plus duplicate checkouts and the >2,000 staged-file boundary locally where suitable.
   - Check: `bun run scripts/testing/github-fixtures.ts --self-test`; the action list equals the dry run; immutable owner and repository IDs, run marker, URLs and seed fingerprints are captured at creation. Before every later seed, push or delete, revalidate those IDs and reject replacement, transfer, rename or marker drift. Creation and push need an explicit mutation flag. Self-tests reject a same-name replacement with a different repository ID.
   - Trap: a name prefix, an owner/name URL or an old ID check alone authorizing an irreversible operation. If the provider cannot bind deletion to the immutable approved resource, cleanup is manual and owner-observed.
3. **Run the full native feature matrix**, Windows first then Linux: secure token restart, all clone modes, stage and full-index commit, separate push, branches, snapshots, set drilldown, editor/hunk/file/folder copy, dirty guards, recycle and persisted recovery.
   - Check: per-row PASS/FAIL/SKIPPED with exact artifact, revision, profile and fixture IDs, and before/after local and remote fingerprints. Remote actions are tested in the foreground, never via prewarm.
4. **Run the adversarial matrices:** interruption, cache-empty, cache-hit, progressive-final, cancellation, resources and performance. Inspect recovery after restart and the GitHub remote after explicit push.
   - Check: full common gates; all native safety and restore tests; ratified release measurement targets on Windows. Note process-kill versus power-loss evidence and Enterprise limits.
5. **Report and clean up:** present findings and the cleanup manifest; get owner acceptance. Keep fixtures until approved. Clean up only exact manifest-owned resources after verifying owner, remote and recovery state and preserved evidence.
   - Check: no unexpected local or remote changes; owner acceptance and action-specific cleanup approval referencing exact immutable IDs; identity and backups revalidated immediately before any action. Unbindable remote deletion stays manual; recovery records are never erased wholesale.

## Done when
- **A1 (native, owner-observed):** the complete feature matrix passes on Windows and Linux with no skipped mandatory rows. Covers R1, I2.
- **A2 (live, owner-observed):** the own-account create, discover, clone, branch, commit, push and restart matrix passes. Covers R2, R4, I1.
- **A3 (native fixtures and release):** adversarial, fault, cache, progressive, performance and resource equivalence. Covers R3, I2.
- **A4 (fixtures, static and live):** approved immutable-ID action and cleanup manifest; drift is rejected; no unrelated changes or secrets. Covers R4, I1, I2.

## Authority and rollback
The owner decides remote creation, seeding, push and deletion, and approves each destructive local action. Dedicated worktree, one writer; the main session integrates. Verify local restore before changes and keep offline seed copies. Local rollback disables new writers but keeps forward recovery. Remote publication and deletion, and eligible-backup cleanup, cannot be undone: a recreated repository gets new IDs and settings. No force push or default deletion is needed.

Stop on the wrong account, extra permissions, manifest drift, a failed restore, unsafe writes or an unavailable required host. Permission to test is not permission to publish a PaperWing release.

## Revision log
- 2026-10-02: Proposed own-account fixture acceptance; no account lookup, repository creation, push or delete during planning.
- [REV 2026-10-02] Independent review: pin fixture authority to immutable owner and repository IDs and reject drift; unbindable remote cleanup is manual and owner-observed.
- 2026-10-05: Windows-first ordering (D4); depends on 21a/21b after the packet 21 split. Rewritten in plain format.
