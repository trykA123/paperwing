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

Moved out: branch cleanup and set search are in 39.

## Search follow-ups (owner, 2026-10-06; wait for the shell pick)
- File-name search across a set (`git ls-files` / `ls-tree` for a ref), same view as code search.
- Several refs per repo in one search (backend already accepts repeated repos with different refs), e.g. all remote branches.
- History search: `git log -S` / `-G` across a set.
- A match limit selector (default caps are 200 per repo, 2000 overall).

## Test stability
- `git::process_tests::linux_post_spawn_capture_failure_retains_registration_and_permits_until_reap` flakes under parallel load (CI run 37573417043, passed on rerun). Make the process tests take `TEST_RUNNER_LOCK` or wait for runner idleness.
