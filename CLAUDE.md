# CLAUDE.md — cargo-polylith

## What This Is

`cargo-polylith` is a Cargo subcommand that brings the polylith architecture
model to Rust/Cargo workspaces. Binary name: `cargo-polylith`. Invoked as
`cargo polylith <command>`.

See `PLAN.md` for the full design and implementation roadmap.

## Build and Test

```bash
cargo build                        # build the binary
cargo test                         # unit + integration tests
cargo run -- polylith --help       # run locally (note: double `polylith` is correct due to cargo subcommand convention)
cargo clippy                       # lint
```

To use as an actual cargo subcommand during development:
```bash
cargo install --path .
cargo polylith --help
```

## Architecture (strict module separation)

- `src/workspace/` — **read-only** analysis: discover Cargo.toml files, build `WorkspaceMap`. Never writes files.
- `src/scaffold/` — **write-only**: create dirs, write template files. No parsing.
- `src/commands/` — thin dispatch between CLI and those two subsystems.
- `src/output/` — all terminal rendering.

## Key Conventions

- `toml_edit` (not `toml`) when writing back to existing Cargo.toml files — preserves comments and formatting.
- `cargo_toml` for typed reading of Cargo.toml files.
- Error handling: `anyhow` for command-level errors, `thiserror` for domain errors in `workspace/`.
- All file templates live in `src/scaffold/templates.rs` as Rust string literals.
- Integration tests use `assert_cmd` + `tempfile` — spawn subprocess, assert files and stdout.

## Cargo Subcommand Pattern

Cargo calls `cargo-polylith polylith <args>` (subcommand name is repeated).
The clap setup uses `#[command(bin_name = "cargo")]` with a `Polylith` variant
in `CargoCommand` to handle this transparently.

## No Compilation of User Code

All analysis is pure TOML + filesystem. No `cargo metadata` invocation.
The tool must work even when the user's workspace doesn't fully compile.

## Primary Test Targets

- `~/projects/modular-digital-music-array` — real-world target (27 components, 11 bases) for `deps` and `info`
- `tests/fixtures/` — minimal hand-crafted polylith workspace for unit/integration tests

## Git Commit Style

Follow Conventional Commits: `feat(component): add new-component scaffolding`
Do NOT include "Co-Authored-By: Claude" lines.
