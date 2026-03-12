# cargo-polylith Implementation Plan

## What This Is

`cargo-polylith` is a Cargo subcommand (`cargo-polylith` binary) that brings
the polylith monorepo architecture model to Rust/Cargo workspaces.

Invoked as: `cargo polylith <command>` (Cargo finds binaries named `cargo-<X>`
and routes `cargo X` to them).

---

## Crate / Dependency Choices

```toml
[package]
name = "cargo-polylith"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "cargo-polylith"
path = "src/main.rs"

[dependencies]
# CLI
clap = { version = "4.5", features = ["derive"] }

# TOML parsing — cargo_toml gives a typed Cargo.toml model for free
cargo_toml = "0.20"

# TOML editing — preserves comments and formatting when writing back
toml_edit = "0.22"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Terminal output
colored = "2.0"

# Filesystem traversal
walkdir = "2.5"

# Serialisation (for --json flag later)
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[dev-dependencies]
tempfile = "3.10"
assert_cmd = "2.0"    # integration test helpers for CLI binaries
predicates = "3.0"    # assertion DSL used with assert_cmd
```

Why `cargo_toml` over raw `toml`? It already parses workspace members,
dependencies, `[patch]` tables, and package metadata into typed structs.
Why `toml_edit` for writes? It preserves comments and formatting when
mutating existing `Cargo.toml` files.

---

## Source Layout

```
cargo-polylith/
  Cargo.toml
  Cargo.lock
  PLAN.md
  CLAUDE.md
  README.md
  devenv.nix
  devenv.yaml
  src/
    main.rs               ← arg dispatch only; delegates to commands::*
    cli.rs                ← clap Cli / Commands / subcommand enums
    commands/
      mod.rs
      init.rs             ← `cargo polylith init`
      component.rs        ← `cargo polylith component new <name>`
      base.rs             ← `cargo polylith base new <name>`
      project.rs          ← `cargo polylith project new <name>`
      deps.rs             ← `cargo polylith deps`
      info.rs             ← `cargo polylith info`
    workspace/
      mod.rs              ← re-exports; entry point for workspace analysis
      discover.rs         ← find & parse Cargo.toml files; build WorkspaceMap
      model.rs            ← types: WorkspaceMap, Brick, BrickKind, DepGraph
    scaffold/
      mod.rs              ← scaffold helpers: create dirs, write template files
      templates.rs        ← file-content templates (lib.rs, main.rs, Cargo.toml)
    output/
      mod.rs
      table.rs            ← tabular/tree renderers for terminal
  tests/
    integration/
      init_test.rs
      new_component_test.rs
      deps_test.rs
      info_test.rs
    fixtures/             ← minimal hand-crafted polylith workspace for tests
```

### Why This Structure?

- `workspace/` is pure analysis — reads files, builds the in-memory model, never writes.
- `scaffold/` is pure writing — creates directories and files. No analysis.
- `commands/` thin glue: parse CLI args → call workspace or scaffold → call output.
- `output/` handles all rendering. Keeps display logic out of commands.

---

## Core Data Model (`src/workspace/model.rs`)

```rust
pub enum BrickKind { Component, Base }

pub struct Brick {
    pub name: String,          // crate name from Cargo.toml [package].name
    pub kind: BrickKind,
    pub path: PathBuf,         // absolute path to the brick directory
    pub deps: Vec<String>,     // crate names this brick depends on
    pub manifest_path: PathBuf,
}

pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub members: Vec<PathBuf>, // base paths listed as workspace members
    pub patches: Vec<(String, PathBuf)>, // (crate-name, alt-impl-path) from [patch]
}

pub struct WorkspaceMap {
    pub root: PathBuf,
    pub components: Vec<Brick>,
    pub bases: Vec<Brick>,
    pub projects: Vec<Project>,
}
```

---

## Discovery Algorithm (`src/workspace/discover.rs`)

1. Accept a `root: PathBuf` (CWD or explicit `--workspace-root`).
2. Walk up from CWD to find the root `Cargo.toml` with `[workspace]` if not given.
3. Scan `components/*/Cargo.toml` — parse each as a `Brick` with `kind = Component`.
4. Scan `bases/*/Cargo.toml` — parse each as a `Brick` with `kind = Base`.
5. Scan `projects/*/Cargo.toml` — parse `[workspace].members` and `[patch]` tables.
6. Return `WorkspaceMap`.

Edge cases: missing dirs (pre-init), partial scaffolds, varied `[patch]` syntax.

---

## Command Implementations

### `cargo polylith init`

1. Warn if `components/`, `bases/`, or `projects/` already exist.
2. Create `components/`, `bases/`, `projects/` directories.
3. Create `.cargo/config.toml` with `[build] target-dir = "target"`.
4. Print next-steps hint.

### `cargo polylith component new <name>`

1. Validate `name` (snake_case, valid Rust ident).
2. Create `components/<name>/src/`.
3. Write `Cargo.toml`, `src/lib.rs` (re-export skeleton), `src/<name>.rs` (impl stub).
4. Add `"components/<name>"` to root `Cargo.toml` `[workspace].members` via `toml_edit`.

`lib.rs` skeleton:
```rust
mod <name>;
pub use <name>::*;
```

### `cargo polylith base new <name>`

Mirror of component: `bases/<name>/src/main.rs`, `[[bin]]` in Cargo.toml.

### `cargo polylith project new <name>`

1. Create `projects/<name>/`.
2. Write `projects/<name>/Cargo.toml` with empty `[workspace]`, commented `[patch]` placeholder.
3. Print hint about adding bases as members.

### `cargo polylith deps`

1. Build `WorkspaceMap`.
2. Walk each base's deps transitively through components.
3. Print dependency tree (or `--json`).
4. Optional: `--component <name>` to show only paths including that component.

### `cargo polylith info`

1. Build `WorkspaceMap`.
2. Print three sections: Components, Bases, Projects.
3. Highlight unused components (depended on by nothing).

---

## Implementation Order

### Phase 1 — Scaffolding (MVP part 1)

1. **Binary skeleton** — `Cargo.toml` + `src/cli.rs` with full clap tree (stubs).
   Verify: `cargo build && cargo polylith --help` shows all commands.
2. **`init`** — directory creation + `.cargo/config.toml`. Integration test.
3. **`component new`** — file templates + root workspace member update. Tests.
4. **`base new`** — mirror of component. Tests.
5. **`project new`** — project workspace manifest. Tests.

**Shippable as v0.1 after Phase 1.**

### Phase 2 — Analysis (MVP part 2)

6. **Full `WorkspaceMap` discovery** — complete `discover.rs` + unit tests with fixtures.
7. **`info`** — tabular renderer. Test against `mdma`.
8. **`deps`** — recursive dep walk + tree renderer. Test against `mdma`.

**MVP complete after Phase 2.**

### Phase 3 — Polish

- `--workspace-root <path>` global flag.
- `--json` on `deps` and `info`.
- Better error messages.
- README.md with usage examples.

### Phase 4 — Post-MVP

- `cargo polylith check` — interface compatibility via `syn` parsing of `lib.rs`.
- `cargo polylith edit` — TUI project composer using `ratatui`.

---

## Key Architectural Decisions

### Cargo subcommand pattern

```rust
// src/cli.rs
#[derive(Parser)]
#[command(bin_name = "cargo")]
pub struct Cargo {
    #[command(subcommand)]
    pub cmd: CargoCommand,
}

#[derive(Subcommand)]
pub enum CargoCommand {
    /// Polylith architecture tools for Cargo workspaces
    Polylith(PolylithArgs),
}
```

Cargo calls `cargo-polylith polylith <args>` — the subcommand name is repeated.
The `bin_name = "cargo"` + `Polylith` variant handles this transparently.

### `toml_edit` for mutations

When appending to `[workspace].members` in root `Cargo.toml`, use `toml_edit`
not `toml` — it preserves comments and formatting.

### No compilation of user code

All analysis is pure TOML + filesystem. No `cargo metadata` invocation. This
means the tool works even when the user's workspace doesn't compile yet.

### Templates as string literals

File templates live in `src/scaffold/templates.rs` as Rust string literals with
`format!()` substitution. No external template engine needed.

---

## Test Strategy

- Unit tests inline (`#[cfg(test)]`) for `workspace/` discovery and model logic.
- Integration tests in `tests/integration/` using `assert_cmd` + `tempfile`:
  spawn `cargo-polylith` subprocess, assert file structure and stdout.
- `tests/fixtures/` — minimal hand-crafted polylith workspace for analysis tests.
- Manual validation against `~/projects/modular-digital-music-array` (27 components, 11 bases).

---

## Immediate First Steps

```bash
cd /home/johlrogge/projects/cargo-polylith
cargo init --name cargo-polylith
# Edit Cargo.toml to add dependencies
# Create src/cli.rs with full clap tree
cargo build
cargo run -- polylith --help
```
