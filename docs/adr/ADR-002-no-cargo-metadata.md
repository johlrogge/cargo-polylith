# ADR-002: Pure TOML+Filesystem Analysis, No `cargo metadata`

## Status
Accepted

## Context
The natural way to understand a Cargo workspace is `cargo metadata`, which provides authoritative dependency and package data. However, `cargo metadata` requires the workspace to compile successfully and invokes the Cargo build system. A key use case for `cargo polylith check` is running it before fixing compilation errors, which `cargo metadata` would block.

## Decision
All workspace analysis is pure TOML parsing and filesystem traversal. `cargo metadata` is never invoked.

## Consequences
The tool works even when the user's workspace doesn't compile — a key use case (running checks before fixing errors). Analysis is significantly faster. No dependency on a specific Cargo version or build environment. Trade-off: some information (e.g. resolved dependency versions) is not available; the tool operates on declared, not resolved, structure.
