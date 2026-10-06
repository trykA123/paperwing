# Packet 16 — Correct metadata caching, private-repo discovery and de-duplicated requests

| | |
|---|---|
| Status | Proposed. Needs owner approval. |
| Weight | Heavy where source, credential or cache contracts change. |
| Depends on | Accepted 03 (state boundaries), 09 (platform paths), 10 (credentials). |
| Primary platform | Windows (work use). Behaviour is platform-neutral; native checks run on Windows first. |
| Read first | Extracted repository-metadata and tree owners; RefPicker and CompareReferencePicker; AppState `refState`; `github.rs` listings and commits; Settings token/source handlers; keyring invalidation seam. |

## Goal
Branch lists, commit histories and repository listings are cached under the right scope and are invalidated correctly. Private repositories in your own GitHub account are discovered. Identical requests in flight are made once. No speculative work and no speed promise before measurement.

## Why
- The commit cache is keyed by repository only, although requests are per branch, so branch B can show branch A's history. `refState` reads the same wrong key.
- The GitHub listing disk cache reuses entries by source ID without checking configuration or age; refs are keyed by URL and trees by path.
- `list_owner` tries the organization listing, then `/users/{owner}/repos`, which never returns private repositories of the signed-in user.

## Scope
- **In:** cache keys, freshness and invalidation; authenticated personal-account discovery; single-flight for foreground metadata.
- **Out:** immutable fact cache (21), heat model, remote prewarm, performance targets.

## Requirements
- **R1 — Commit history keys** include source, repository, requested branch and ref epoch. Every reader uses one shared helper; errors can be retried.
- **R2 — Scope and freshness** of listings, refs and trees are explicit. Force refresh and changes of source, token, ref, root or repository invalidate correctly. Late responses cannot refill an obsolete scope.
- **R3 — Shared requests** have per-consumer lifecycle, retry cleanup and tested bounds. One consumer cancelling or closing never fails another.
- **R4 — Personal discovery.** A configured authenticated personal-owner source lists that owner's private and public repositories, with correct pagination and permission handling. Unrelated owners never appear, and organization, manual and other-user listings keep their current behaviour.
- **I1** — A timer is not proof of correctness. Comparison and write paths still validate freshly. A denied or unavailable credential never leaves unauthorized listings shown as fresh.
- **I2** — Credential values or hashes, file content and write authority are never persisted, cached or logged. No background network requests are added.

## Decisions
- **D1** — Start with simple versioned keys, epochs, explicit freshness and single-flight. The old five-minute and 30-second values are proposals; ratify timings from packet 01 measurements.
- **D2** — The private-listing API contract (candidate `/user/repos` with ownership filter) must be verified against dated official GitHub and GitHub Enterprise documentation before implementation. Do not infer it from source.
- **D3** — Old cache versions are treated as misses, never migrated.

## Steps
1. **Define the key/freshness/invalidation table** and shared helper types, including the non-secret credential revision from 10 and the physical repository identity from 09. List every reader with `rg` before changing storage.
   - Check: key-matrix fixtures for duplicate checkouts and branches, edited source host and organizations, token revision, ref movement and root replacement.
2. **Add the personal-owner listing route** after verifying the official contracts. Determine the signed-in owner from a validated profile response. Keep organization, other-user and manual routes and the Repo identity compatible. Never silently replace required discovery with manual URLs.
   - Where: `github.rs::{GhOwner, GhLogin, list_owner, get_json, list_repos}`, a controlled HTTP test seam, source-scope keys.
   - Check: controlled HTTP fixtures for private and public own repositories, several pages, owner filtering, token denial/expiry/insufficient permission, organization fallback, another user's public repositories and an Enterprise base URL. Live private discovery is proven again in 23.
   - Stop if: unrelated repositories appear in an owner's listing, or missing permissions are hidden by a public-only fallback.
3. **Fix per-branch histories and retries.** Update `ensureCommits`, `refState`, RefPicker and all cache readers together. Keep the selected ref during a valid refresh.
   - Check: `bun test src/lib`, including simultaneous branch A/B requests, a failed retry, an out-of-order old epoch and unchanged IPC branch arguments.
4. **Version the listing disk cache** and apply the ratified non-secret fingerprint, revision and age gates, plus denied or unavailable credential-store behaviour. Invalidate refs and trees on force refresh, focus and relevant Git operations.
   - Check: native isolated-cache fixtures with source and token edits, old-version and corrupt entries, permission denial, replaced root, external branch and file changes, force refresh.
   - Trap: serving stale permission-scoped lists after a token or store error, or treating a path string or timer as validated identity.
5. **Add bounded single-flight** with per-consumer interest, cancellation and failure cleanup. Shared work survives one waiter cancelling; zero-consumer cleanup and retries are explicit.
   - Check: concurrent and cancelled waiters, late generations, error floods and exact request counts; native picker and open-tab behaviour; process memory.

## Done when
- **A1 (fixtures):** all keys and readers handle branch, source, ref, credential and root changes and retries. Covers R1, R2, I1, I2.
- **A2 (fixtures and native):** single-flight cancellation, bounds and cleanup hold, and selection is preserved. Covers R3, I1, I2.
- **A3 (native release, Windows):** navigation and filtering use already-loaded state with zero unsolicited network; repeat request counts fall without a cold regression. Covers R2, R3, I2.
- **A4 (controlled HTTP fixtures; live proof in 23):** private personal discovery, pagination, permissions and ownership work, and other routes are unchanged. Covers R4, I1, I2.

## Authority and rollback
Common frontend, build, serial Cargo, Clippy and diff gates; packet 01 replay runner and native isolated profile. The owner approves cache-format changes and credential-adjacent invalidation policy. One writer works in a dedicated worktree; the main session integrates. Protect original settings and tokens; prove a restore of a sacrificial cache and configuration. Rollback bypasses or clears disposable metadata cache entries only, never credentials, repositories or recovery data.

Stop on scope leakage, stale authority, secret exposure, unbounded producers or an unaccepted migration.

## Revision log
- 2026-10-02: Proposed metadata behaviour changes, separate from state extraction and prewarm.
- [REV 2026-10-02] Independent review: own the authenticated private personal-account discovery contract needed by 23 here, rather than repairing it during acceptance.
- 2026-10-05: Rewritten in plain format; Windows-first native checks. No change in scope.
