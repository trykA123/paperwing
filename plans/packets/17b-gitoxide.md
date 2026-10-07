# 17b — Cold path, phase 2: gitoxide object reads

Status: ready after packet 17 is re-measured on Windows (a diagnostics export from the owner's work PC). Optional.
Platform: Windows first, Linux parity
Size: M
Role: api-builder (gpt-6.1-sol xhigh), one writer. Optional: ships only if it beats phase 1.

## Goal
Read blobs and trees in-process with gitoxide (`gix`) so a cold comparison spawns no Git process for object access. Adopt it only where measurements beat phase 1, especially on Windows with Defender.

## Already done
Nothing. Packet 17 phase 1 (on main; see git history for its packet) provides the `BatchReader` interface in `src-tauri/src/git/batch.rs` and the equivalence oracle in `src-tauri/src/compare/tests/legacy/`.

## Decisions
- `gix` is pinned to an exact version and approved as a new dependency in this packet. Install from crates.io only, with `Cargo.lock` committed.
- Read-only: blobs and trees, and tree diffs. Never writes, credentials, network, hooks or filters.
- It sits behind the same reader interface as the batched Git path. Batched Git stays as the fallback when `gix` cannot open a repository (unusual object stores, alternates, unsupported formats) and for any normalised comparison it cannot match exactly.
- A bounded worker count (at most the smaller of available parallelism and 4) runs the CPU work. Use Rayon only if measurement proves it.
- A setting or build flag can switch back to batched Git.

## Scope
- Do: a `gix` reader implementing the phase 1 interface; benchmarks against phase 1 on adversarial fixtures.
- Do not: replace Git for writes, status, checkout or network; change comparison semantics.

## Read first
- Packet 17's measurement table (`git log --all -- plans/packets/17-cold-path.md`)
- `src-tauri/src/git/batch.rs`, `src-tauri/src/compare/inventory.rs`, `text_diff.rs`
- `src-tauri/Cargo.toml`

## Steps
1. Spike: add `gix` behind a feature, implement blob reads and tree diffs, and run the legacy oracle. Check: every fixture matches (EOL, whitespace, long lines, binary, type changes, SHA-256 repositories, submodules).
2. Benchmark against phase 1 on the Windows VM with Defender on, and on Linux: p50 and p95 latency, process count, memory, binary size, compile time. Check: the table is in the commit message.
3. Decide with the owner. Keep it only if it wins on Windows and compile time and binary size grow by less than 20 percent.
4. Wire cancellation and fallback. Check: cancellation under load still stops work within the existing p95; a forced `gix` failure falls back to batched Git with identical output.

## Done when
- A measured Windows win over phase 1, or the packet closes as "not adopted" with the numbers recorded.
- Output is identical to the oracle for all options.

## Gates
- `bun run --bun check`, `bun test src/lib`, `bun run --bun build`
- `cd src-tauri && cargo test --offline`, `rustfmt --check`, clippy; Windows VM serial `cargo test --locked`

## Stop and report if
- Any result or count differs, cancellation cannot interrupt the work, or foreground contention gets worse.
- The dependency pulls in network, TLS or credential code that cannot be disabled.

## Report
Commit sha, benchmark table, adopt or not adopt, gate results.
