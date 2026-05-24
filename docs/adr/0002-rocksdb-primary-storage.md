# ADR-0002: RocksDB Primary Storage

## Context
Synapse M3 needs one implemented storage backend for local runtime state, profile
support, reflex audit rows, and MCP tools. Earlier docs kept a `sled-backend`
escape valve for Windows RocksDB risk, but no sled backend implementation
exists in the codebase.

Local dependency audit on 2026-05-24 showed the unused optional `sled`
dependency pulled in `tempdir`, `remove_dir_all`, and `bincode` advisories.
Keeping an unavailable fallback in the manifest made the dependency graph less
secure without adding runtime capability.

## Decision
RocksDB is the only M3 storage backend. Remove the unused `sled-backend` feature
and the `sled` dependency. A future fallback backend requires a new issue, a
maintained dependency graph, an implemented storage adapter, and manual
source-of-truth verification on this configured Windows host.

## Consequences
- Positive: `cargo audit` no longer reports the sled transitive advisories.
- Positive: docs and manifests match the implemented system.
- Negative: there is no pure-Rust storage escape valve in M3.
- Trade-off accepted: if RocksDB becomes unreliable on this host, the fix is a
  fresh implementation issue rather than an advertised but nonexistent feature.

## References
- Issue: #335
- Doctrine: #351
