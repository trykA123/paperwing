# Skein entropy repair

This directory vendors the published `dbus-secret-service` 4.1.0 crate from
[crates.io](https://crates.io/crates/dbus-secret-service/4.1.0). The original archive's
SHA-256 is `708b509edf7889e53d7efb0ffadd994cc6c2345ccb62f55cfd6b0682165e4fa6`.
All 22 original archived files matched the cached registry source before editing.
The original `LICENSE-APACHE`, `LICENSE-MIT`, copyright notices, source comments,
`Cargo.toml.orig` and registry VCS metadata are preserved. The registry-generated
`.cargo-checksum.json` is omitted because these sources are modified.

The patch replaces `fastrand` with exact `getrandom` 0.3.4, using its `std` feature.
The pinned getrandom archive's SHA-256 is
`899def5c37c4fd7b2664648c28120ecec138e4d395b459e5ca34f9cce2dd77fd`.
Both `crypto-rust` and `crypto-openssl` obtain DH private exponent bytes and CBC IVs
from the operating system. Entropy failures return the existing `Error::Crypto`
before session negotiation or secret mutation. Public signatures, DH parameters,
HKDF, cipher choice, padding and Secret Service wire encoding are unchanged.
One redundant pair of parentheses in the existing `Error::Crypto` type is removed
to clear an upstream compiler warning exposed by using a local path dependency.

Four tests under `session::crypto::entropy_regressions` exercise OS-backed DH key
exchange, CBC round trips and injected partial entropy failures. They use no D-Bus
connection, wallet or user credentials. Existing upstream tests access Secret
Service, so run only the bounded filter from the application directory:

```sh
cargo test --locked --offline -p dbus-secret-service --lib entropy_regressions -- --test-threads=1
```

`Cargo.toml.orig` and the bundled upstream `Cargo.lock` are historical archive files.
The application resolves the patched `Cargo.toml` through its own `Cargo.lock`.
