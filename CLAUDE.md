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

## Polylith Is for Applications, Not Published Crates

`cargo-polylith` is intentionally structured as a **flat single-crate workspace** and must stay that way — and this is the *correct* choice, not a limitation.

**Polylith shines for applications** where you own all the code, build everything in one workspace, and never need to publish internal components to crates.io. The single-workspace model with path dependencies is exactly the right fit for that context.

**"Use before reuse"**: prove a component's general value inside the workspace first, across multiple bases and projects. Only once that value is proven does it make sense to promote the component to a standalone published crate. This is a natural graduation path — not a failure of the model.

`cargo-polylith` itself is already at that graduation point: it IS a published crate (`cargo install cargo-polylith`). Path dependencies between internal components would require publishing each as a separate crate with a globally unique name — significant release complexity with no meaningful benefit for a single-binary tool.

A future roadmap item will allow importing a crates.io library to act as a component behind a named interface — the inverse direction.

Do not attempt to migrate this project to polylith architecture.

## No Compilation of User Code

All analysis is pure TOML + filesystem. No `cargo metadata` invocation.
The tool must work even when the user's workspace doesn't fully compile.

## Primary Test Targets

- `~/projects/modular-digital-music-array` — real-world target (27 components, 11 bases) for `deps` and `info`
- `tests/fixtures/` — minimal hand-crafted polylith workspace for unit/integration tests

## Git Commit Style

Follow Conventional Commits: `feat(component): add new-component scaffolding`
Do NOT include "Co-Authored-By: Claude" lines.
