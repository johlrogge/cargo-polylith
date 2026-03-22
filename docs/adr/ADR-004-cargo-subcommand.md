# ADR-004: Cargo Subcommand CLI Pattern

## Status
Accepted

## Context
Rust ecosystem tools that extend Cargo are conventionally distributed as `cargo-<name>` binaries, making them invocable as `cargo <name>`. This is the established pattern for tools like `cargo-expand`, `cargo-edit`, etc. Distributing as a standalone binary with a different invocation name would break ecosystem expectations and make discovery harder.

## Decision
The binary is named `cargo-polylith` and registers itself via clap with `#[command(bin_name = "cargo")]` and a `Polylith` subcommand variant. This means Cargo calls `cargo-polylith polylith <args>` (the subcommand name is repeated), which clap handles transparently.

## Consequences
Users invoke the tool as `cargo polylith <command>`, which feels native to the Rust toolchain. The repeated subcommand name in argv is an implementation detail invisible to users but must be understood by contributors. Installation via `cargo install cargo-polylith` integrates naturally with standard Rust toolchain management.
