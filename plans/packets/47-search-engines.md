# 47 — Search engines and fuzzy file finder, chosen in Settings

Status: ready (after 43)
Platform: Windows first, Linux parity
Size: L
Role: both (backend first: api-builder; then ui-builder for Settings and the finder)

## Goal
Code search runs without starting a process per repository, and the user picks the engine in Settings. A new "Go to file" finder (Ctrl+P) fuzzy-matches file names across every repository in the active set. On the owner's work PC a Git spawn costs 150-320 ms, so a 23-repo search pays that 23 times today.

## Already done
- Code search: `src-tauri/src/search.rs` runs `git grep` per repository through the runner, streams per-repo events, enforces caps, cancel and cancel_all. Flags `-i`, `-w`, `-F`/`-G`/`-P`, and an opt-in `untracked` option.
- Fuzzy matching (fzf-style, frontend only): `src/lib/fuzzy.ts`, used by `CommandPalette.svelte`, `palette.ts`, `Select.svelte`, `RefSelect.svelte`.
- Settings persistence: `settings.rs` (settings.json), Settings UI in the shell from packet 44.

## Decisions
- Settings › Search has three choices:
  1. **Code search engine:** `Built-in` (default) or `Git grep`. Built-in uses ripgrep's libraries in-process (`grep-searcher`, `grep-regex`, `grep-matcher`, `ignore`; MIT/Unlicense), never a bundled `rg.exe`. Git grep is today's path, kept unchanged as the fallback.
  2. **Files searched:** `Tracked files` (default, same result set as `git grep`) or `Tracked and untracked` (working tree minus `.gitignore`, `.git/info/exclude` and the global excludes).
  3. **Finder matching:** `Fuzzy` (default, fzf-style) or `Exact substring`. Applies to the command palette, pickers and the new file finder.
- Built-in "Tracked files" takes the file list from the Git index read once per repository (one `git ls-files -z` call, or the index directly if gitoxide is already a dependency); it must return the same files as `git grep` for the parity fixture.
- Built-in search is Unicode-aware for `-i`, `-w` and regex in every locale. PCRE (`-P`) stays Git-grep-only: with the built-in engine, a `-P` request runs through Git grep for that search and the UI says so.
- Same request and event contract as today (`search.rs` types); the engine is chosen inside the backend. No new IPC shape for search; the finder gets its own command.
- File finder backend: walk with `ignore` (parallel), match with `nucleo` (MPL-2.0; the Helix fzf-style matcher) in Rust, stream the top results; respects the Files searched setting. Frontend shows results with matched positions highlighted, using the existing palette styles.
- Deletes, writes and remote calls: none. Read-only feature.

## Scope
- Do: the built-in engine behind a trait next to the Git grep engine; the three settings with defaults and migration-free reads (missing key = default); the file finder command and Ctrl+P UI; parity and Unicode tests; a benchmark entry (`search-code` in the benchmark OPERATIONS list).
- Do not: GitHub-wide search (packet 45), content indexing or a persistent index, replacing `git grep` in other places, new search UI beyond the engine note and the finder.

## Read first
- `src-tauri/src/search.rs`, `src-tauri/src/search_tests.rs`
- `src-tauri/src/git/runner.rs`, `git/locale.rs` (packet 43)
- `src/lib/fuzzy.ts`, `src/components/CommandPalette.svelte`, `src/lib/palette.ts`
- `src-tauri/src/settings.rs`, the Settings component from packet 44
- `~/.agents/rules/rust.md`, `typescript.md`, `web-ui.md`, `code-quality.md`

## Steps
1. Engine trait and Git grep engine (pure move, no behaviour change). Check: existing search tests pass unchanged.
2. Built-in engine, tracked files. Check: parity fixture (tracked, ignored, untracked, binary, large file, submodule, CRLF, non-ASCII paths) returns identical hits to `git grep` for `-F`, `-G`, `-i`, `-w`; caps and cancel tests pass on both engines.
3. Unicode: `-i 'ș'` matches `Ș`, `-w` respects non-ASCII word boundaries, independent of the process locale.
4. Untracked option on the built-in engine. Check: `.gitignore`, nested ignores and `info/exclude` respected.
5. Settings keys and UI. Check: switching engine changes the next search; a `-P` search with Built-in shows the fallback note.
6. File finder command (`ignore` + `nucleo`, streamed, cancellable) and Ctrl+P UI. Check: 20k-file fixture returns first results under 150 ms on Linux; matched positions highlighted.
7. Finder matching setting wired into palette, pickers and finder.
8. Benchmark: add `search-code` and `find-file` to the OPERATIONS list.

## Done when
- With Built-in, a set search starts zero Git processes when no `-P` is used (diagnostics export shows no grep spawns).
- Results match Git grep on the parity fixture.
- Windows CI `test-windows` passes; on the owner's work PC a 23-repo search is visibly faster than Git grep.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `bun scripts/testing/css-order.ts` (UI changes; `--write` regenerates `src/styles/order.json`)
- `cd src-tauri && cargo test --offline` (set `SKEIN_TEST_TMP` to an ext4 directory, never `/tmp`)
- `rustfmt --check` and clippy on touched Rust files
- UI: screenshots at 1440 and 1100 px, both themes

## Stop and report if
- The built-in engine cannot match `git grep`'s tracked-file set without spawning Git per repository.
- New crates pull in a licence other than MIT, Apache-2.0, Unlicense, BSD or MPL-2.0.
