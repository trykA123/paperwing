# Metadata caching

Metadata loads start from foreground actions. They share work within each metadata owner;
closing a picker or tree releases only that consumer. A producer keeps its admission slot
until the IPC call finishes. An abandoned response cannot publish. Errors remain retryable.
Each owner admits at most 32 producers and 256 consumers and reports excess admission as busy.

| Metadata | Versioned scope | Freshness and invalidation |
| --- | --- | --- |
| Listings | Source ID, kind, host, owners, credential management and credential revision | Source or token edits and credential-store failures invalidate. A missing token does not. Disk rows are keyed on cache version and source configuration, store the login they were fetched for, and are shown as stale on startup. Every load then revalidates in the foreground (`GET /user` and listing) and replaces the rows. Offline or failed revalidation keeps the stale rows. |
| Histories | Source configuration and revision, repository, requested branch and ref epoch | Ref selection changes the requested branch scope. Forced refresh and source/token edits invalidate. Focus and Git operation status refresh mark entries stale: the last data stays visible and open pickers reload it. |
| Remote refs | URL, all owning source configurations/revisions and ref epoch | Force refresh and source/token edits invalidate. Focus marks entries stale; badges keep the last value and pickers re-request. A failed reload keeps the stale entry. |
| Trees | Path, destination root, observed root/path identity, native Git/common-directory identity and generation | Root changes, focus, Git operation status refresh and explicit refresh invalidate. Credential revisions also invalidate trees for that source’s set items. New physical path observations invalidate immediately. Native responses bind root, `.git`, Git directory and common directory identities before and after reading. |

Freshness uses explicit epochs and stale marks. No new timer lifetime is
assumed, and cached metadata does not grant comparison or write authority. No background
network requests, credential values or credential hashes are added to cache keys or disk rows.
Listing responses with errors never refill disk. Newer generations reject older successes
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

The cache is disposable. Rollback can remove `repos-<source-id>.json` from the application
cache after closing the app; credentials, settings and repositories do not need changes.
