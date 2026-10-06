# 23 — Native acceptance with disposable GitHub fixtures

Status: blocked by 17, 20, 21a (and 06 for the Windows write rows)
Platform: Windows first (Defender on), then Linux. Windows results gate acceptance.
Size: L
Role: orchestrator plus api-builder; the owner observes live steps

## Goal
Prove every feature end to end on native Windows and Linux with local adversarial fixtures and disposable repositories in the owner's own GitHub account. This tests existing workflows; it adds no repository creation or deletion to Skein.

## Already done
- `scripts/testing/github-fixtures.sh` creates private repos `skein-fixture-api`, `-web`, `-infra` and `-mono` on the signed-in `gh` account and skips existing ones. `--delete` removes them (needs `delete_repo` scope).
- `scripts/testing/{baseline,fixtures,native-profile}.ts` provide the baseline runner, local fixtures and an isolated native profile.
- A Windows VM exists (`plans/2026-10-06/handoff.md`). CI builds `Skein.exe` and a Linux deb and AppImage.
- Enterprise host and multi-source behaviour is covered by fixture tests only (`src-tauri/src/github/`, `src/lib/api-pulls.test.js`).

## Decisions
- Disposable repositories live only in the owner's own account. Planning and agents never create or delete remote resources without the owner approving that exact action.
- Remote authority is pinned to immutable owner and repository ids captured at creation. Before any seed, push or delete, revalidate them and reject replacement, transfer, rename or marker drift. If deletion cannot be bound to the immutable resource, cleanup is manual and owner-observed. A name prefix alone never authorises an irreversible step.
- Never hardcode github.com. Resolve the host from the remote and the configured source.
- Enterprise: several company hosts plus github.com matter to the owner. Own-account testing cannot prove Enterprise behaviour. Test host parsing, auth and request contracts with fixtures and label live Enterprise evidence as missing, or have the owner run a read-only pass on the work PC.
- No force push, no token-scope expansion, no default repository deletion, no speculative network. Push is a separate user action after commit.
- Windows first, Defender on. Every file or folder change uses the accepted protections and a verified fixture restore. A skipped required Windows or Linux row blocks "parity complete".
- Use an isolated profile, root and keyring namespace. Never connect a test profile to real checkout roots. No credentials in files.

## Scope
- Do: the full feature matrix on both platforms; own-account discovery, clone, branch, commit and explicit push; fault, cache, progressive and performance replays; a cleanup manifest.
- Do not: new features, live Enterprise testing by agents, release publication.

## Read first
- `scripts/testing/github-fixtures.sh`, `scripts/testing/native-profile.ts`, `docs/testing.md`
- `docs/implementation-status.md`, `plans/2026-10-06/handoff.md`
- Evidence from 17, 19, 20, 21 and the Windows rows of 14 and 15

## Steps
1. Confirm the plan: signed-in owner, visibility, run id, fixture names, local roots, profile and credential mechanism, and the exact create, seed, push and delete actions. Produce a dry-run manifest. Check: the owner accepts the actions; the account is never inferred from a repository origin.
2. Create and seed only the approved repositories with deterministic branches, tags, divergence, renames, EOL, whitespace, binary, large text, type, mode, link, submodule and destination-only cases, plus duplicate checkouts and a staging boundary above 2,000 files. Check: ids, URLs and seed fingerprints are recorded; later actions match the dry run; a same-name replacement with a different id is rejected.
3. Run the native matrix, Windows first: secure token restart, all clone modes, stage and commit, separate push, branches, tags, stash, snapshots, set drilldown, editor, hunk, file and folder copy, dirty guards, recycle and persisted recovery. Check: a PASS, FAIL or SKIPPED row per feature with artifact, revision, profile and fixture ids and local and remote fingerprints. Remote actions run in the foreground, never through prewarm.
4. Run the adversarial matrices: interruption, cache-empty, cache-hit, progressive-final, cancellation, resources, performance. Check: recovery inspected after restart; the GitHub remote inspected after the explicit push.
5. Report and clean up. Check: owner acceptance; cleanup only of manifest-owned resources after revalidating ids and recovery state; recovery records are never erased wholesale.

## Done when
- The full matrix passes on Windows and Linux with no skipped mandatory row.
- The own-account create, discover, clone, branch, commit, push and restart flow passes.
- Drift is rejected; nothing unrelated changed; no secret appeared in logs.

## Gates
- `bun run --bun check`, `bun test src/lib`, `cd src-tauri && cargo test --offline`
- Windows VM serial `cargo test --locked`; owner-observed walkthrough in the production WebView

## Stop and report if
- The wrong account, extra permissions, manifest drift, a failed restore, an unsafe write or an unavailable required host.
- Permission to test never means permission to publish a release.

## Report
The per-row matrix, fixture ids, cleanup manifest, gate results, what stayed unproven (live Enterprise).
