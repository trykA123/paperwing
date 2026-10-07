# Search backend settings

Search preferences persist in `settings.json` under `workspace`. Missing keys use the defaults below. Existing workspace values remain intact. Invalid values produce an error when starting a search.

| Key | Values | Default |
| --- | --- | --- |
| `searchEngine` | `builtIn`, `gitGrep` | `builtIn` |
| `searchFiles` | `tracked`, `trackedAndUntracked` | `tracked` |
| `finderMatching` | `fuzzy`, `exactSubstring` | `fuzzy` |

The backend reads preferences for each new search. An existing request with `untracked: true` continues to include untracked files.

Built-in search uses one cancellable `git ls-files` command per repository to read indexed paths and apply Git path filters. Content matching uses ripgrep libraries inside the application. Untracked searches walk the admitted paths in parallel with `ignore`, respecting Git ignores, info excludes and global excludes. Tracked-only searches include force-added ignored files; untracked searches exclude ignored files, matching the existing Git grep behavior.

Git grep remains available as the selected engine. Built-in requests also use Git grep for Perl expressions, committed refs, backreferences and other unsupported basic-regex constructs. A repository completion event includes optional `status.engineNote` when a fallback occurs. Existing search requests and event names remain intact.
