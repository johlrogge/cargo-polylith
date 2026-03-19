---
description: Polylith workspace advisor — analyze structure, guide components/bases/projects
---

# Polylith Workspace Advisor

You are helping with a polylith-architecture Cargo workspace managed by `cargo-polylith`.

## The polylith model

- **Component** — encapsulated domain logic. `src/lib.rs` re-exports a public interface from
  private submodules. Swappable at build time via `[patch]` — no traits or generics needed.
- **Base** — thin wiring layer (HTTP server, CLI, worker, etc.). Depends on components; exposes
  its runtime API as `src/lib.rs`. Never a standalone binary.
- **Project** — deployment unit. A Cargo workspace under `projects/<name>/` that selects one or
  more bases + the components they need, producing real binaries.
- **Development workspace** — the repo root `Cargo.toml` listing all components and bases as
  members, for IDE support and `cargo check`. Not deployable.

## Directory layout

```
repo-root/
  Cargo.toml                     ← dev workspace (all members)
  .cargo/config.toml             ← build.target-dir = "target"
  components/<name>/
    Cargo.toml                   ← [package.metadata.polylith] interface = "<name>"
    src/lib.rs                   ← pub use re-exports only (no implementation)
    src/<impl>.rs                ← private implementation
  bases/<name>/
    Cargo.toml
    src/lib.rs                   ← public runtime API
  projects/<name>/
    Cargo.toml                   ← project workspace + [workspace.dependencies]
```

## Swappable implementations

Multiple components can share the same `name` in their `Cargo.toml`. Projects select which
path wins via `[workspace.dependencies]`:

```toml
# projects/prod/Cargo.toml
[workspace.dependencies]
user = { path = "../../components/user" }

# projects/test/Cargo.toml
[workspace.dependencies]
user = { path = "../../components/user_stub" }
```

No traits needed — the compiler enforces the interface. Missing or mismatched functions are
compile errors at every call site.

## Getting live workspace data

If the `cargo-polylith` MCP server is active, call these tools before answering:

- `polylith_info` — all components, bases, projects and their declared deps
- `polylith_deps` — dependency graph; pass `component` to filter by a specific component
- `polylith_check` — structural violations (errors and warnings)
- `polylith_status` — lenient audit with observations and suggestions

If MCP is unavailable, fall back to the CLI:

```bash
cargo polylith info
cargo polylith deps [--component <name>]
cargo polylith check
cargo polylith status
```

## Scaffolding commands

```bash
cargo polylith component new <name> [--interface <iface>]
cargo polylith base new <name>
cargo polylith project new <name>
```

## Rules to enforce

1. A component's `lib.rs` contains **only** `pub use` re-exports — never implementation code.
2. Prefer named re-exports (`pub use foo::{A, B}`) over wildcards (`pub use foo::*`).
3. Bases never depend on other bases — only on components.
4. Projects contain no business logic — wiring and deployment configuration only.
5. Every component should declare `[package.metadata.polylith] interface = "<name>"` so
   `cargo polylith deps` and `cargo polylith check` can reason about swap groups.

## Your task

$ARGUMENTS
