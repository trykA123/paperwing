# Skein backlog (approved ideas, no packet yet)

- Sync a branch with main across a set: merge or rebase with a previewed plan ("Plan sync" from the Lanes prototype).
- Three-way merge editor for pulls that cannot fast-forward (shares the engine in 37).
- Export a compare report as HTML or CSV.
- Open a repository in a file manager (terminal and VS Code are in 03b and the row menu).
- Run a command across a set with per-repository output.
- Submodule and Git LFS awareness.
- File history and blame in the history drawer.
- Other hosting providers (Azure DevOps, GitLab, Bitbucket) as providers on 38, if needed at work.
- Jenkins as a second `CiProvider` after 31.
- Squash-merge detection for branch cleanup (39 states the gap).
- Compare: revoke-and-restart batch readers after fetch instead of permanent close (packet 17 round 2 review)

Moved out: branch cleanup and set search are in 39.

## Search follow-ups (owner, 2026-10-06; wait for the shell pick)
- File-name search across a set (`git ls-files` / `ls-tree` for a ref), same view as code search.
- Several refs per repo in one search (backend already accepts repeated repos with different refs), e.g. all remote branches.
- History search: `git log -S` / `-G` across a set.
- A match limit selector (default caps are 200 per repo, 2000 overall).

## Test stability
- `git::process_tests::linux_post_spawn_capture_failure_retains_registration_and_permits_until_reap` flakes under parallel load (CI run 37573417043, passed on rerun). Make the process tests take `TEST_RUNNER_LOCK` or wait for runner idleness.

## Packet 44 follow-ups (pass 2 review, 2026-10-08)
- Shell cleanup: delete the dead right-panel CSS (`.item-details`, `.set-summary`, `.tree-*` rules in `workflow-overrides.css` and `workspace.css`) and rename the drawer `history-*` classes to `details-*` (update `ui-26`, `ui-33` and `ui-44` selectors with them).
- Compare details panel: move comparison rules and Copy to left/right out of the right grid track (packet 37), then drop the track, `rightVisible` and `rightWidth`.
- Unbuilt in the repository shell: clone by URL ("Clone repository…"), discard in the Changes section (packet 29 UI), failing runs in the quick look and run details in the drawer (packet 31 run data), pull requests for remote-only repositories (needs a remote lookup, not a local path).
- Chip counts on the Repositories home are a lower bound until statuses are read (shown with a "+"). A cheap on-disk probe (`paths_exist`) could give exact "Cloned" counts without full status.
- Packet 38 nit: the events bus serialises each event twice (`RawValue`); skip the second pass when no bus subscribers are registered.
- Dev warning `ownership_invalid_binding` when Code search opens: `CodeSearch.svelte` does `bind:form={session.form}` on a session that `App.svelte` passes as a plain prop. Fix the ownership (bindable prop or a function binding) and check the console in `ui-39`.
- `ui-44` fails at 390 px (Ctrl+1–5, Ctrl+J, Enter on an item, page overflow, table width 310 vs 770): its width asserts assume 1440, and the 250 px sidebar crowds a narrow window. Decide a minimum window width or make the asserts width-aware.
- `ui-39` "2000 matches stays smooth" frame check flakes under heavy machine load (133 ms worst frame with other harnesses running). Run harnesses sequentially.

## Packet 30 follow-ups (third review, 2026-10-08; Windows CI must compile and test the watcher before push)
- Slow network shares: an unreachable share can stall a page reload while the 60-second health check runs.
- Junction roots: repointing a watched junction to a new target goes unnoticed.
- Changes Windows drops are caught only when the window regains focus, not on the repository page.
- Mixed watch types: a folder watched both with and without subfolders can lose its subfolders (unusual Git folder layouts only).
