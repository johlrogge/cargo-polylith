---
description: Polylith workspace advisor — analyze structure, guide components/bases/projects
---

# Polylith Workspace Advisor

You are helping with a polylith-architecture Cargo workspace managed by `cargo-polylith`.

## The polylith model

- **Component** — encapsulated domain logic. `src/lib.rs` re-exports a public interface from
  private submodules. Swappable at build time via path dependencies — no traits or generics needed.
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

Components implementing the same interface have the same `interface` name in their metadata
but different package names (e.g. `user` and `user-stub`). Projects select which
implementation is active by declaring a path dependency aliased to the interface name:

```toml
# projects/prod/Cargo.toml — use the real implementation
[dependencies]
user = { path = "../../components/user" }
# package = omitted: the crate is already named "user"

# projects/bdd/Cargo.toml — use the stub
[dependencies]
user = { path = "../../components/user_stub", package = "user-stub" }
# package = required: the stub crate is named "user-stub", not "user"
```

All code in both projects calls `use user::UserService;` identically. The compiler enforces
that both components expose the same public API — mismatched functions are compile errors.

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
cargo polylith check [--profile <name>]
cargo polylith status
```

## Profiles

A **profile** is a named set of interface-to-implementation mappings. Rather than
directly editing `projects/<name>/Cargo.toml` each time you want to swap components,
you define profiles once and activate them at build time.

```toml
# .polylith/profiles.toml (example)
[profiles.prod]
user = "components/user"
storage = "components/storage-postgres"

[profiles.bdd]
user = "components/user-stub"
storage = "components/storage-memory"
```

Profile commands:

```bash
cargo polylith profile list [--json]              # list defined profiles and their mappings
cargo polylith profile build <name> [--no-build]  # activate a profile (patch Cargo.tomls); --no-build patches only
cargo polylith profile add <interface> \
  --impl <path> --profile <name>                  # add or update one mapping in a profile
cargo polylith check --profile <name>             # validate the workspace as if profile <name> were active
```

`profile build` resolves each mapping and rewrites the relevant `[dependencies]`
entries in the development workspace and/or project `Cargo.toml` files. Use
`--no-build` to stage the file changes without invoking `cargo build`.

`check --profile <name>` validates profile-specific concerns — including
`profile_impl_path_not_found` and `profile_impl_not_a_component` — in addition
to the standard structural checks.

## Scaffolding

**Prefer MCP write tools** when the server was started with `--write`. Always confirm with
the user before creating or modifying workspace structure.

| MCP tool | Purpose | Required params |
|---|---|---|
| `polylith_component_new` | Create component under `components/<name>/` | `name`; optional `interface` (defaults to name) |
| `polylith_base_new` | Create base under `bases/<name>/` | `name` |
| `polylith_project_new` | Create project workspace under `projects/<name>/` | `name` |
| `polylith_component_update` | Set or update interface annotation on existing component | `name`, `interface` |
| `polylith_set_implementation` | Wire a component implementation into a project | `project`, `interface`, `implementation` |

If MCP write tools are unavailable (server started without `--write`, or MCP not active),
fall back to the CLI equivalents:

```bash
cargo polylith component new <name> [--interface <iface>]
cargo polylith base new <name>
cargo polylith project new <name>
```

## Violation model

`polylith_check` (and `cargo polylith check`) reports violations in two categories:

**Hard errors** — non-zero exit, must be fixed:
- `dep-key-mismatch` — a path dep key in `[dependencies]` doesn't match the crate's
  `package.name` and no `package = "..."` alias was provided. Fix: add
  `package = "<real-crate-name>"` to the dep entry, or rename the dep key to match.
- `profile_impl_path_not_found` — a profile entry references a component path that
  does not exist on disk. Fix: correct the path in `.polylith/profiles.toml` or
  create the missing component.
- `profile_impl_not_a_component` — a profile entry references a path that exists but
  is not a recognised workspace component (missing `[package.metadata.polylith]` or
  not listed as a workspace member). Fix: add the correct metadata or point the
  profile at a proper component directory.

**Warnings** — exit 0, flagged for attention:
- `hardwired_dep` — a component or base declares a direct `path = "..."` dependency
  to another workspace component instead of using `{ workspace = true }`. This
  bypasses the swap mechanism and makes the implementation non-swappable. Fix:
  move the dep to `[workspace.dependencies]` in the root `Cargo.toml` and reference
  it as `<name> = { workspace = true }` in the component/base.
- `ProjectFeatureDrift` — a project's external dep declares fewer features than the root
  workspace dep. Fix: add the missing features to the project's dep declaration.
- `ProjectVersionDrift` — a project's external dep version differs from the workspace.
  Fix: align the version with the root workspace.
- `WildcardReExport` — `pub use foo::*` in a component's `lib.rs`; prefer named re-exports.
- `OrphanComponent` — component not used by any project.
- Other structural warnings: `MissingInterface`, `AmbiguousInterface`, `DuplicateName`,
  `ProjectMissingBase`, `NotInRootWorkspace`, `BaseHasMainRs`.

## Rules to enforce

1. A component's `lib.rs` contains **only** `pub use` re-exports — never implementation code.
2. Prefer named re-exports (`pub use foo::{A, B}`) over wildcards (`pub use foo::*`).
3. Bases may depend on other bases, but prefer components for shared logic.
4. Projects contain no business logic — wiring and deployment configuration only.
5. Every component should declare `[package.metadata.polylith] interface = "<name>"` so
   `cargo polylith deps` and `cargo polylith check` can reason about swap groups.

## Your task

$ARGUMENTS
