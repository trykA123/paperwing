# Packet 38 — stopped before implementation

Base: `de9d1f5`, branch `crew/api-builder-p38-core`. No commits, merges, pushes, deployment, servers, or source edits.

The explicit run rule requires stopping when a task assumption is wrong. The supplied checkout has **74 default command names**, plus **8 optional names**. Its registration list contains **102 entries**, including three platform alternatives for each of ten file commands. It does not contain 104 command names. The packet says “about 104”; the run instruction says “All 104”. The orchestrator must reconcile that requirement with the actual baseline before implementation resumes.

## Steps

| Step | Change | Check |
| --- | --- | --- |
| 1 | No crate extraction. Comparison's `Job` directly stores `crate::linux_diff::Storage` and `crate::linux_guard::root::RootValue`; comparison tests also directly use protected file modules. Moving the full module requires changing protected platform boundaries. The authorized fallback keeps steps 2–6 inside the app crate. | Baseline workspace tests ran; results below. Split/build checks skipped. |
| 2 | Not implemented after the command-count stop condition. | Before/after registration inventory generated; empty diff. |
| 3 | Not implemented. | Event forwarding compatibility test skipped; frontend event code unchanged. |
| 4 | Not implemented. | Existing GitHub tests ran in the baseline suite; lifecycle/counting-transport tests skipped. |
| 5 | Not implemented. | Persistence and Settings/browser checks skipped. |
| 6 | Not implemented. | Architecture document/checklist deferred. |

## Commands and evidence

- `git status --short`, `git worktree list`: confirmed the designated worktree and branch; only `node_modules` was initially untracked.
- `df -T .`: confirmed the worktree is on ext4.
- `SKEIN_TEST_TMP="$PWD/.packet38/test-tmp" TMPDIR="$PWD/.packet38/test-tmp" cargo test --offline --workspace --manifest-path src-tauri/Cargo.toml`: exit **101**; **609 tests: 599 passed, 3 failed, 7 ignored**. Both temporary paths were absolute, inside this worktree, with no `..` and no `/tmp`.
- Python inventory extraction from `lib.rs` before and after inspection: **82 unique names including optional features; 102 raw entries; diff empty**. No command signatures, arguments, results, or event names changed.
- `git diff --name-only`: empty source diff.
- `cargo clippy --offline --workspace --all-targets`, rustfmt, `bun test src/lib`, `bun run --bun check`, `bun run --bun build`, Linux `tauri build --no-bundle`, Windows CI, compile-time/binary-size comparison: **skipped after the explicit stop condition**. No implementation gate is claimed as passing.

Files in `packet38-core-boundaries-evidence/` contain the baseline test log, command inventories, empty command diff, and original command signatures.

## Baseline environment failures

1. `linux_diff::tests::foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace`: `newuidmap: Could not set caps`.
2. `linux_guard::tests::access_acl_roundtrip_preserves_the_kernel_validated_metadata`: ACL setup returns `EINVAL` (`Invalid argument`).
3. `linux_journal::tests::native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled`: ACL setup returns `EINVAL` (`Invalid argument`).

## Platform and risks

Moved `#[cfg(windows)]` items: **none**. All platform blocks and protected files remain untouched. No new host constant or provider implementation was introduced.

Packet 38 remains unimplemented. Resume against the actual 74 default/82 feature-inclusive names, using the authorized app-crate fallback unless the orchestrator supplies an extraction boundary that preserves the protected platform modules.
