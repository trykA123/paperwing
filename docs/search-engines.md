# Search backend settings

Search preferences persist in `settings.json` under `workspace`. Missing keys use the defaults below. Existing workspace values remain intact. Invalid values produce an error when starting a search.

| Key | Values | Default |
| --- | --- | --- |
| `searchEngine` | `builtIn`, `gitGrep` | `builtIn` |
| `searchFiles` | `tracked`, `trackedAndUntracked` | `tracked` |
| `finderMatching` | `fuzzy`, `exactSubstring` | `fuzzy` |

The backend reads preferences for each new search. An existing request with `untracked: true` continues to include untracked files.

Built-in search uses one cancellable `git ls-files` command per repository to read indexed paths and apply Git path filters. Content matching uses ripgrep libraries inside the application. Untracked searches walk the admitted paths in parallel with `ignore`, respecting Git ignores, info excludes and global excludes. Tracked-only searches include force-added ignored files; untracked searches exclude ignored files, matching the existing Git grep behavior.

Git grep remains available as the selected engine. Built-in requests also use Git grep for Perl expressions, committed refs, backreferences and other unsupported basic-regex constructs. Repositories with relevant `.gitattributes`, metadata `info/attributes`, global attribute files, or attribute/include configuration also use Git grep. This conservative fallback preserves Git binary-file rules and can reduce the built-in performance gain. A repository completion event includes optional `status.engineNote` when a fallback occurs. Existing search requests and event names remain intact.

## File finder commands

`finder_start` accepts `{ repos: string[], query: string, maxResults?: number }` and returns a job id. The default result limit is 100; the maximum is 500. Use existing `search_cancel` or `search_cancel_all` to cancel finder jobs. The code search and finder share the same four-job registry.

`finder-matches` carries `{ id, sequence, matches }`. Each snapshot replaces the previous results for that job; sequence numbers increase. Results contain `{ repo, path, score, positions }`, sorted by descending score, then repository and path. Positions identify UTF-16 code units for frontend highlighting. `finder-done` carries `{ id, summary }`, where the summary includes scanned files, total matches, cancellation and per-repository errors.

Fuzzy matching treats query words as subsequences with Nucleo's path scoring. Exact substring matching treats the entire query literally, including spaces. Both modes ignore case. File names use forward slashes on every platform.

Attribute detection checks working-tree ancestors, listed index paths, repository and common metadata, HOME/XDG user locations, configured global/system Git config paths, and common system installation locations. Filtered code-search listings also admit top-level and nested attribute paths from the index; detected attributes trigger fallback before scanning, using the original filters. Detection does not resolve arbitrary Git installation prefixes; select `gitGrep` when such system attributes apply.

Benchmark builds record complete search jobs as `ipc.content/search-code` and finder jobs as `ipc.files/find-file`. The baseline trace parser accepts both operations. These timings do not increment Git command counts.
