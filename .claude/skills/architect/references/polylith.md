# Polylith Architecture in Rust

Polylith is a monorepo architecture for maximum component reuse across multiple deployable artifacts.
Originally from Clojure; this document describes the Rust/Cargo mapping.

## Core Concepts

### Component
A Cargo library crate under `components/<name>/`. Its **interface is the public surface of `lib.rs`** — the `pub` items re-exported from private submodules. Nothing beyond the crate boundary is accessible to other bricks.

```
components/user/
  Cargo.toml
  src/
    lib.rs        ← ONLY pub use re-exports
    user.rs       ← private implementation
```

```rust
// src/lib.rs — the interface: what other bricks can see
mod user;
pub use user::create_user;
pub use user::get_user;
pub use user::User;
```

```rust
// src/user.rs — the implementation: private
pub fn create_user(email: &str) -> Result<User> { ... }
pub fn get_user(id: UserId) -> Result<User> { ... }
```

**No traits required.** The interface is plain named functions, as Joakim Tengstrand (polylith inventor) intends. The crate's pub surface IS the interface contract.

### Interface metadata
Cargo-polylith uses an explicit metadata declaration in each component's `Cargo.toml`:

```toml
[package.metadata.polylith]
interface = "user"
```

New components get this automatically (`component new` defaults `interface` to the crate name).
Existing components can be updated with `component update <name> [--interface <NAME>]`.
`cargo polylith check` warns for any component missing this declaration.

### Base
A Cargo **library crate** under `bases/<name>/`. Exposes a runtime API (HTTP server, CLI, IPC, gRPC …) as ordinary Rust functions (`run()`, `serve()`, `create_sockets()`). Bases wire components together but do not hardcode which implementations are used.

**Bases must NOT have `src/main.rs`.** If a base were a binary, two bases could never share one process.

```
bases/http_api/
  Cargo.toml
  src/
    lib.rs        ← pub fn serve(...) / run(...) / create_sockets(...)
    handler.rs    ← private implementation
```

### Project
A Cargo **bin crate** under `projects/<name>/`. Listed as a member of the root workspace. Owns `src/main.rs` and calls the bases' runtime-API functions. Projects CAN depend on components directly (valid polylith). Projects MUST depend on at least one base.

```
projects/production/
  Cargo.toml      ← [package] + [[bin]] only; NO [workspace] section
  src/main.rs     ← entry point: calls base fns, wires components
```

A project has **no domain logic** — all logic lives in bricks. The `main.rs` is a thin wiring point.

Projects are **NOT** workspace roots. They are plain bin crates listed in the root `[workspace] members`. This is the correct polylith model: one workspace, many deployable artifacts.

### Development Workspace
The repo root `Cargo.toml`. Lists ALL components, bases, and projects as members. Used for `cargo check`, IDE support, day-to-day development, and building any project. Not just a development artifact — it IS the workspace.

## Directory Layout

```
repo-root/
  Cargo.toml              ← root workspace: members = all components + bases + projects
  components/             ← library crates (NOT a workspace root)
    user/
      Cargo.toml
      src/lib.rs          ← pub re-exports only
      src/user.rs         ← private impl
    user_inmemory/        ← alternative implementation (same crate name "user")
      Cargo.toml
      src/lib.rs
      src/user.rs
  bases/                  ← runtime-API library crates (lib only, no main.rs)
    http_api/
      Cargo.toml
      src/lib.rs          ← pub fn serve(...)
  projects/
    production/
      Cargo.toml          ← [package] + [[bin]] only; NO [workspace] section
      src/main.rs         ← entry point: calls base fns
    test-env/
      Cargo.toml          ← different implementation choices via package = "..."
      src/main.rs
```

## `cargo polylith check` Violations

| Violation                | Kind    | Exit |
|--------------------------|---------|------|
| Component missing lib.rs    | error   | 1    |
| Base missing lib.rs         | error   | 1    |
| Base has main.rs            | warning | 0    |
| Base depends on base        | error   | 1    |
| Project has no base dep     | warning | 0    |
| Component not reachable     | warning | 0    |
| Wildcard re-export          | warning | 0    |
| Missing interface metadata  | warning | 0    |
| Ambiguous interface         | warning | 0    |
| Duplicate package name      | warning | 0    |
| Not in workspace members    | warning | 0    |

Projects depending directly on components is valid and not flagged.

## Swappable Implementations

Alternative implementations of the same component share the same Cargo package name:

```toml
# components/user_inmemory/Cargo.toml
[package]
name = "user"             # same name as components/user/
version = "0.1.0"
```

The compiler enforces compatibility — if a function is missing or has the wrong signature, it is a compile error everywhere that function is called. This is Rust's type system doing the interface checking, with no traits involved.

### How Projects Select Implementations

Projects are plain bin crates in the root workspace. Each project selects which implementation of a component to use by listing direct path dependencies with the `package = "..."` alias to resolve the correct crate:

```toml
# bases/http_api/Cargo.toml
[dependencies]
user = { path = "../../components/user" }   # direct path dep; name matches package name
```

```toml
# projects/production/Cargo.toml  (NO [workspace] section)
[package]
name = "production"
version = "0.1.0"

[[bin]]
name = "production"
path = "src/main.rs"

[dependencies]
http_api = { path = "../../bases/http_api" }
# Override: swap in the Postgres user implementation
user     = { path = "../../components/user" }
```

```toml
# projects/test-env/Cargo.toml  (NO [workspace] section)
[package]
name = "test-env"
version = "0.1.0"

[[bin]]
name = "test-env"
path = "src/main.rs"

[dependencies]
http_api = { path = "../../bases/http_api" }
# Override: swap in the in-memory implementation using package alias
user     = { path = "../../components/user_inmemory", package = "user_inmemory" }
```

Build a specific project binary with:
```bash
cargo build --bin production
```

## Project Structure

A project is a plain bin crate that is a member of the root workspace. It has NO `[workspace]` section of its own.

```toml
# projects/production/Cargo.toml
[package]
name = "production"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "production"
path = "src/main.rs"

[dependencies]
http_api = { path = "../../bases/http_api" }
cli      = { path = "../../bases/cli" }
user     = { path = "../../components/user" }
library  = { path = "../../components/library_service" }
```

The root workspace `Cargo.toml` lists projects as members:

```toml
# Cargo.toml (repo root)
[workspace]
members = [
  "components/*",
  "bases/*",
  "projects/*",
]
resolver = "2"
```

Each project selects its implementations by listing direct path dependencies. To swap an implementation, use `package = "..."` aliasing:

```toml
# Alternative implementation: user_inmemory crate exposed as "user"
user = { path = "../../components/user_inmemory", package = "user_inmemory" }
```

## The Root Workspace

The repo root workspace lists all components, bases, and projects:

```toml
# Cargo.toml (repo root)
[workspace]
members = [
  "components/*",
  "bases/*",
  "projects/*",
]
resolver = "2"
```

This gives full IDE support and lets you run `cargo check` and `cargo build` across the entire codebase. There is only one workspace — projects are members of it, not separate workspaces.

## What the cargo-polylith Tool Does

Managing this structure by hand is tedious. `cargo-polylith` handles:

- **Scaffolding**: `cargo polylith component new <name> [--interface <NAME>]` — always creates interface metadata (defaults to crate name)
- **Interface update**: `cargo polylith component update <name> [--interface <NAME>]` — set/replace interface on an existing component
- **Base scaffolding**: `cargo polylith base new <name>` creates `bases/<name>/` with `lib.rs` (pub fn run() skeleton) and Cargo.toml
- **Project management**: `cargo polylith project new <name>` generates the project workspace manifest
- **Overview**: `cargo polylith deps` shows which components are used by which bases and projects
- **Interface checking**: `cargo polylith check` verifies structural correctness and reports violations
- **Interactive editor**: `cargo polylith edit` — TUI to toggle project/component connections, set interface names ('i' key), write all staged changes to disk ('w')

## Polylith in mdma

The `modular-digital-music-array` project at `~/projects/modular-digital-music-array` is the primary migration target. It has 27 components and 11 bases, plus a `projects/` layer with 9 projects.

Current status:
- Most bases are correctly lib crates: `http-server` exposes `serve()`, `service` exposes `create_sockets()`
- `mdma-library` base incorrectly has `main.rs` instead of `lib.rs` (flagged as warning by `cargo polylith check`)
- Three projects (`mdma-cli`, `mdma-gateway`, `mdma-tui`) have no base dependency yet (flagged as warnings)
- All components currently have no interface metadata — `cargo polylith check` will produce 27+ `missing-interface` warnings until `[package.metadata.polylith] interface` is added to each

`cargo polylith check` reports these violations to guide the migration.
