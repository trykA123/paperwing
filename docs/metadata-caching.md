# Metadata caching

Metadata loads start from foreground actions. They share work within each metadata owner;
closing a picker or tree releases only that consumer. A producer keeps its admission slot
until the IPC call finishes. An abandoned response cannot publish. Errors remain retryable.
Each owner admits at most 32 producers and 256 consumers and reports excess admission as busy.

| Metadata | Versioned scope | Freshness and invalidation |
| --- | --- | --- |
| Listings | Source ID, kind, host, owners, credential management and credential revision | Source or token edits and credential-store failures invalidate. A missing token does not. Rows live in the local SQLite store (`skein-store.sqlite3` in the app data directory) and are keyed on cache version and source configuration, store the login they were fetched for, and are shown as stale on startup. Every load then revalidates in the foreground (`GET /user` and listing) and replaces the rows. Offline or failed revalidation keeps the stale rows. |
| Histories | Source configuration and revision, repository, requested branch and ref epoch | Ref selection changes the requested branch scope. Forced refresh and source/token edits invalidate. Focus and Git operation status refresh mark entries stale: the last data stays visible and open pickers reload it. |
| Remote refs | URL, all owning source configurations/revisions and ref epoch | Force refresh and source/token edits invalidate. Focus marks entries stale; badges keep the last value and pickers re-request. A failed reload keeps the stale entry. |
| Trees | Path, destination root, observed root/path identity, native Git/common-directory identity and generation | Root changes, focus, Git operation status refresh and explicit refresh invalidate. Credential revisions also invalidate trees for that source’s set items. New physical path observations invalidate immediately. Native responses bind root, `.git`, Git directory and common directory identities before and after reading. |

Freshness uses explicit epochs and stale marks. No new timer lifetime is
assumed, and cached metadata does not grant comparison or write authority. No background
network requests, credential values or credential hashes are added to cache keys or disk rows.
Listing responses with errors never refill disk. A listing that lost later pages of an owner (`partial`, with per-owner `warnings`) is stored marked partial, so the next start shows it as stale rows; it never replaces a fuller stored listing, and the next load revalidates it. Owners are listed at most four at a time. Newer generations reject older successes
and failures at publication, including native disk publication after forced refresh.

With a credential, discovery validates `GET /user`. When a configured owner matches that
login, it requests `/user/repos?affiliation=owner&visibility=all&per_page=100&page=N`.
It checks every repository owner and reports incomplete discovery at 100 pages.
Other owners retain organization listing and first-page 404 fallback to public user listing.
401/403 responses cannot trigger public fallback. HTTP requests pin API version `2022-11-28`,
reject redirects and validate pagination origin, endpoint and sequential page numbers.

The controlled transport fixtures use synthetic responses without network or a credential store.
Live token permissions, Enterprise server compatibility and Windows native acceptance require
the separate native acceptance run. Fine-grained tokens expose only their granted repositories.

The cache is disposable. Repository listings are stored in `skein-store.sqlite3` in the application data directory. Commit histories are also written to the store but not yet read back. Rollback: close the app and delete `skein-store.sqlite3` with its `-wal`, `-shm` and `.running` files. Credentials, settings and repositories need no change. Legacy `repos-<source-id>.json` files in the application cache directory are deleted at startup.

The store opens on a background thread. Until it is ready, and whenever it is disabled, every read is a cache miss and every write is dropped. A file is moved aside only when SQLite reports it is not a database or is corrupt, or a migration fails; open, I/O, permission, read-only and busy errors disable the store for the session and leave the file untouched. A full integrity check runs only after an unclean shutdown (the `.running` marker is still present).

Known gaps against packet 34 (not implemented yet):

- Commit history `recall` is unused, and `REF_EPOCH` is a constant, so cached commits are never served.
- Pruning removes the oldest `fetched_at` rows first. It is not LRU.
- Vacuum runs only when the file is over the size cap, not on idle.
- The schema version is tracked with `PRAGMA user_version`.
