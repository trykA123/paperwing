# Linux credentials

Packet 10 is implemented and tested on native Linux. The owner approved the backend,
dependencies and private wallet drills on2026-10-03. Independent review approved the
entropy and runtime repairs. Native Windows and owner-observed acceptance remain pending.

## Backend and readiness

Linux uses the persistent desktop Secret Service with RustCrypto. Windows retains keyring
3.6.3, Windows Credential Manager and the existing `paperwing`/legacy `flock` namespaces.
There is no plaintext fallback, token cache, homemade encryption or broad migration.

```toml
[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "=3.6.3", features = ["sync-secret-service", "crypto-rust"] }
dbus-secret-service = { version = "=4.1.0", features = ["crypto-rust"] }
dbus = "=0.9.12"
```

The common keyring Windows/Apple features remain unchanged. Cargo.lock adds 33 feature
packages without changing existing versions or checksums. The Bun lockfile is unchanged.
The reference host already provides libdbus 1.16.2 and KWallet 6.30.0; no OS package was
installed for this packet.

PaperWing vendors the pinned synchronous backend at
[`src-tauri/vendor/dbus-secret-service`](../src-tauri/vendor/dbus-secret-service/PAPERWING-PATCH.md)
through a Cargo path patch. The published 4.1.0 source used non-cryptographic `fastrand`
for DH private exponents and CBC initialization vectors. The patch uses exact
`getrandom` 0.3.4 with its `std` feature for both crypto variants. Its
[`fill` API](https://docs.rs/getrandom/0.3.4/getrandom/fn.fill.html) returns OS entropy
errors; these become the existing `Error::Crypto` before negotiation or secret writes.
There is no entropy fallback. Public method signatures and the Secret Service wire
protocol remain unchanged. The patch preserves the original dual licenses and records
the verified archive checksums. The application lockfile reuses the already locked
getrandom package; the backend becomes a local path package with that dependency.

The bounded `entropy_regressions` library-test filter checks OS-backed DH key exchange,
CBC round trips and partial entropy failures without opening D-Bus or a wallet.
Never run the upstream crate's unfiltered test suite against the desktop secret store.

The [keyring 3.6.3 features](https://docs.rs/crate/keyring/3.6.3/features) and
[pinned backend source](https://docs.rs/keyring/3.6.3/src/keyring/secret_service.rs.html)
identify direct Secret Service persistence. The maintained adapter's exact source was
checked against [official registry metadata](https://crates.io/api/v1/crates/dbus-secret-service/4.1.0)
before installation. Linux uses `connect_with_max_prompt_timeout(Dh, 0)` so operations do
not unlock or create a wallet automatically. A locked store requires the user to unlock
it through their desktop wallet and retry. Each library RPC has a separate two-second
timeout; this is not a deadline or cancellation guarantee for a complete operation.

Readiness probes inspect service ownership, the default collection and its lock state.
They do not read secrets or activate a desktop wallet. This desktop currently has no
`org.freedesktop.secrets` owner, so the production app honestly reports unavailable
storage. Installed KSecrets alone does not establish a configured, accessible store.
Manual repository sources remain usable without it.

## Ownership and contracts

`src-tauri/src/credentials.rs` owns admission, native operations, sanitized failures and
nonsecret source revisions. Settings retains persistence and compatibility commands.
Credential tasks run outside the GUI thread, with one active operation, at most 32
admission waiters and a two-second admission timeout.

Platform reporting adds `credentials:{backend,persistent,supported,reason}` independently
of filesystem capabilities. `credential_status(sourceId)` returns
`{sourceId,backend,state,revision,reason}`. States distinguish saved, missing, locked,
unavailable, permissionDenied, uncertain and error. Existing set/delete/has-token
arguments and successful payloads remain compatible. Inaccessible storage rejects
`has_token` rather than reporting an absent token.

Linux retains keyring attributes `service`, `username` and `target=default`. Test-profile
builds use only `paperwing-testing-fixtures-v1` and validated fixture UUIDs. Windows legacy
fallback occurs only when the current entry is absent. Provider errors do not trigger a
fallback or expose raw SDK messages.

Token mutation advances a source revision before dispatch, clears that source's backend
cache and emits `credential-changed`. Source kind, host, organizations or URLs also advance
the revision. Cache publication and renderer requests reject older revisions. The additive
`source_revision` command synchronizes configuration changes before loading fresh data.
No token or token hash enters a revision, event, settings record or cache key.

A dispatched mutation can commit before its response is lost. Such failures retain an
uncertain state until an explicit successful save or delete. Presence cannot identify
which token was committed. There is no compensating write/delete or automatic retry.
Uncertainty and revisions are process-local; restarting does not preserve the entered
draft or a durable mutation journal.

The Settings form preserves entered tokens and drafts after failed saves, failed source
removal and failed new-source cleanup. A nonsecret optional `credentialManaged` flag
retains cleanup ownership if a source becomes Manual. Legacy source DTOs omit this field
and round-trip unchanged. New Manual sources require no wallet operation. GitHub readers propagate store errors before HTTP requests. Git captures configured
credential owners before spawning. Managed Manual sources remain
owners. An inaccessible owner selects quiet execution: local Git still runs, but Activity
arguments, streams and observers are omitted. Successful captures remain attached to
display metadata and errors after token replacement. Typed action identities and file
content stay exact. A changed credential revision is rejected after acquisition and
before HTTP dispatch.

Packet 16 uses this revision seam to invalidate source listings, references and branch-scoped histories.
See [metadata caching](metadata-caching.md) for cache scope, freshness and request lifetimes.

## Native evidence

The opt-in [native runner](../scripts/testing/linux-credentials.py) uses a fresh marked
profile, private D-Bus, private accessibility bus and private XDG wallet storage. HOME and
the real desktop wallet remain untouched. A native Wayland KSecrets dialog creates and
unlocks only the private wallet. Passwords and tokens originate in controller memory;
the app receives tokens over stdin, never argv, environment or persisted fixtures.

The original `10/n4/credential-acceptance.json` and fresh combined-source
`10/n5/credential-acceptance.json` record:

- Save/read across separate application processes and a stopped/restarted native daemon.
- Locked-store overwrite refusal, unlock/reconnect and preservation of the original token.
- Distinct native D-Bus access denial, delete/recreate and repeated delete.
- Authenticated loopback HTTP success, API 401/403 and zero HTTP calls when storage is
  locked or unavailable.
- Settings restoration and generated-secret absence across settings, caches, reports and
  encrypted wallet bytes. Only the generated test identity is deleted.

Reports and sacrificial profiles remain under ignored `.skillify/evidence/paperwing/10/`.
The offscreen wallet experiment failed to expose an accessible dialog; native Wayland
succeeded. Neither mocked IPC nor this private test establishes a configured production
wallet or native Windows acceptance.

Bun tests, Svelte checks and default/test-profile Rust suites pass. Real Helium checks at
390px/1440px in light/dark cover locked and uncertain states, failed deletion/cleanup and
Manual sources without a store. A reproduced Tooltip render-mutation error was fixed and
those browser cases now report no errors. An existing zero-timeout test raced command
completion; its waiting helper now makes the timeout outcome deterministic.
Strict Clippy retains the existing Linux dead-code failures; diagnostic Clippy allows only
that baseline lint. The normal artifact excludes private-drill hooks.

## Rollback and remaining gates

Restore only packet-owned source, lock and UI changes. Delete only generated fixture
credential IDs in their private wallet. Never export or migrate real credentials.
The earlier disposable preview exited with allocator heap corruption during this
session. Its old packet09 artifact has no available core dump; the cause remains
unconfirmed. The rebuilt artifact has separate native smoke evidence. Do not interpret
a short launch check as a fix or a long-running stability guarantee.

Independent review approved the entropy and runtime repairs. The vendored4.1.0
Secret Service adapter uses fallible operating-system entropy for DH keys and CBC IVs.
Six runtime regressions fail before their fixes;65 default/70 test-profile tests and
16 browser cases pass in the isolated runtime worktree. Combined-source80/85 Rust
tests,63 frontend tests and the fresh private KSecrets n5 drill pass. Two native
comparisons preserve the baseline result hash and all Git command counts. The normal
Linux artifact excludes all private test hooks. Windows persistence/restart and
owner-observed desktop workflows remain unavailable or pending. Linux writes still
require packets12–15.
