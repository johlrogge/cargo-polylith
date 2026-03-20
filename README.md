# cargo-polylith

A Cargo subcommand that brings the [polylith](https://polylith.gitbook.io/polylith/) monorepo
architecture model to Rust/Cargo workspaces.

All analysis is pure TOML + filesystem — no `cargo metadata` invocation, so the tool works
even when the workspace doesn't fully compile.

See [VISION.md](VISION.md) for the philosophy behind cargo-polylith and how we adapt
Polylith's model for Rust.

---

## Why Polylith?

Rust developers typically reach for **traits and generics** when they need swappable
behaviour. Polylith offers a complementary approach: swap entire component implementations
at build time, with no runtime overhead.

|  | `dyn Trait` | generics | Polylith swap |
|---|---|---|---|
| Abstraction | yes | yes | yes |
| Dispatch | vtable (runtime) | monomorphized | direct call |
| Heap pressure | `Box<dyn>` per value | none | none |
| Caller complexity | leaks `dyn`/`Arc` | bounds infect every caller | none — plain functions |
| Multiple impls active simultaneously | yes | yes | no (one per binary) |

The limitation — one implementation per binary — is rarely a constraint for application
code. Most "I need swappable storage/transport/client" situations in applications are
actually build-time choices: tests use in-memory, production uses Postgres. Polylith makes
that explicit and free.

When you do need multiple implementations active simultaneously at runtime (e.g. routing
to different backends in the same process), traits are the right tool. Polylith and traits
are complementary, not competing.

See [VISION.md](VISION.md) for a deeper treatment of when each approach fits.

---

## The polylith model — our Rust interpretation

Polylith organises code into four building blocks:

**Components** — library crates that implement a named interface. Each component declares its
interface via `[package.metadata.polylith] interface = "<name>"` in its `Cargo.toml`. Multiple
components may implement the same interface name; exactly one is active in any given build
context (selected by path dependency in a project workspace).

**Bases** — entry-point crates that wire components into a runnable program. Bases depend on
components and expose a library API (`src/lib.rs`) so that project workspace manifests can call
their `run()` function from a thin `src/main.rs`. Bases must not have their own `src/main.rs`.

**Projects** — standalone Cargo workspaces in `projects/`. Each project is an independently
buildable workspace that selects a specific combination of bases and component implementations
via path dependencies in `[dependencies]`.

**Development project** — the root workspace itself. It contains all components and bases and
is optimised for fast feedback during development and testing. Components in the root workspace
default to lightweight implementations (stubs, in-memory versions). Projects select
production-grade implementations as needed.

**Interfaces** — named contracts declared in `[package.metadata.polylith]`. Two components with
the same interface name are alternative implementations. The Rust compiler enforces type
compatibility when an implementation is swapped; `cargo polylith check` performs a structural
pre-check on public symbol names.

---

## Adaptations from the Clojure reference implementation

| Concept | Clojure polylith | cargo-polylith | Why |
|---|---|---|---|
| **Interface declaration** | Namespace structure — components implement an interface by having the same namespace | `[package.metadata.polylith] interface = "..."` in Cargo.toml | Rust has no namespace-based interface; explicit metadata is unambiguous and prevents typos from creating phantom interfaces |
| **Profile / implementation switching** | Named profiles in `deps.edn` select which source directories are compiled in | Path dependency aliased to the interface name in a project's `[dependencies]`; `package = "..."` when the crate name differs | Cargo path deps are the natural mechanism — no indirection through a registry needed |
| **Development project** | A dedicated `development/` project at the workspace root | The root workspace itself | Cargo's workspace model is already the right structure; no separate project needed |
| **Stub-first development** | Default profile uses the primary implementation | Root workspace uses lightweight/stub components by default; projects select production-grade implementations via path deps | Enables fast tests without heavy deps (PipeWire, file scanning, sha2, etc.) |
| **Test/dev projects** | The development project has no base requirement | Projects in `projects/` that are test or development harnesses do not require a base dependency | A test runner is an entry point in its own right; forcing a base dependency would be artificial |
| **Interface compatibility** | `poly` tool checks that components implementing the same interface have matching public APIs | Rust compiler enforces compatibility when you swap implementations; `cargo polylith check` does a structural pre-check on public symbol names | Rust's type system is more expressive than namespace-based interfaces — let the compiler do the definitive check |
| **One interface, two implementations without a default** | Both implementations live in the workspace; profiles select | Both components have the same interface name but neither has a package name matching the interface — every consumer must explicitly declare which to use | Makes the choice intentional rather than implicit; the tool warns `AmbiguousInterface` |

---

## Installation

```bash
cargo install --path .
```

Or, once published:

```bash
cargo install cargo-polylith
```

---

## Quick start

```bash
# 1. Create a new Cargo workspace
cargo new --lib my-mono
cd my-mono

# Turn Cargo.toml into a workspace manifest
echo '[workspace]\nmembers = []\nresolver = "2"' > Cargo.toml

# 2. Initialise polylith structure
cargo polylith init
# → creates components/, bases/, projects/, .cargo/config.toml

# 3. Add a component
cargo polylith component new logger
# → components/logger/Cargo.toml
# → components/logger/src/lib.rs

# 4. Add a base (entry point)
cargo polylith base new api
# → bases/api/Cargo.toml
# → bases/api/src/lib.rs   (exposes run())

# 5. Create a deployable project
cargo polylith project new production
# → projects/production/Cargo.toml  (standalone workspace, add [dependencies] for impls)

# 6. Inspect the workspace
cargo polylith info
cargo polylith deps

# 7. Validate structure
cargo polylith check
```

---

## Commands

### `cargo polylith init`

Initialises a Cargo workspace as a polylith monorepo.

```
cargo polylith init
```

Creates:
- `components/` — home for component crates
- `bases/` — home for base (entry-point library) crates
- `projects/` — home for project workspace manifests
- `.cargo/config.toml` — sets shared `target/` dir

Warns (but does not fail) if any of those directories already exist.

---

### `cargo polylith component new <name>`

Creates a new component crate.

```
cargo polylith component new payment
cargo polylith component new payment --interface payment-service
```

The `--interface` flag sets the interface name written to `[package.metadata.polylith]`; defaults to
the crate name when omitted.

Produces:

```
components/payment/
  Cargo.toml   ← includes [package.metadata.polylith] interface = "payment"
  src/
    lib.rs     ← implementation stub
```

Also appends `"components/payment"` to `[workspace].members` in the root `Cargo.toml`
using `toml_edit` (preserves existing comments and formatting).

---

### `cargo polylith component update <name>`

Sets or replaces the interface annotation on an existing component's `Cargo.toml`.

```
cargo polylith component update payment
cargo polylith component update payment --interface payment-service
```

Defaults to the crate name when `--interface` is omitted.

---

### `cargo polylith base new <name>`

Creates a new base (entry-point library) crate.

```
cargo polylith base new rest-api
```

Produces:

```
bases/rest-api/
  Cargo.toml
  src/
    lib.rs    ← exposes run() — called by a project's thin main.rs
```

Also appends `"bases/rest-api"` to `[workspace].members`.

---

### `cargo polylith project new <name>`

Creates a project workspace manifest.

```
cargo polylith project new production
```

Produces:

```
projects/production/
  Cargo.toml     ← standalone [workspace], add [dependencies] for component impls
```

Edit the manifest to add the bases you want to ship and declare which component
implementation to use for each interface:

```toml
[dependencies]
# Real implementation — package name matches the interface name, no package = needed.
library-service = { path = "../../components/library_service" }

# Or use the stub — package name differs, so alias it to the interface name:
# library-service = { path = "../../components/library_service_stub",
#                     package = "library-service-stub" }
```

---

### `cargo polylith info`

Shows a summary of all bricks in the workspace.

```
cargo polylith info
```

```
Components
  logger
  parser
  payment
Bases
  rest-api
Projects
  production
```

Flags:
- `--json` — machine-readable output

---

### `cargo polylith deps`

Shows the dependency graph from bases down through components.

```
cargo polylith deps
```

```
rest-api (base)
  └─ payment
  └─ logger
```

Flags:
- `--component <name>` — show only bases that depend on a specific component
- `--json` — machine-readable output

---

### `cargo polylith check`

Validates the workspace structure and reports violations.

```
cargo polylith check
```

**Hard violations** (exit 1):

| Tag | Description |
|---|---|
| `missing-lib` | Component or base has no `src/lib.rs` |
| `missing-impl` | Component has no `src/lib.rs` AND no `src/<name>.rs` |
| `dep-key-mismatch` | A path dependency key does not match the target crate's `package.name` — use the correct name as the dep key, or add `package = "..."` as an alias |

**Warnings** (exit 0):

| Tag | Description |
|---|---|
| `orphan` | Component is not reachable from any base or project (including as a swapped implementation via `package =`) |
| `wildcard` | Component's `lib.rs` uses `pub use <crate>::*` — prefer explicit re-exports |
| `base-has-main` | A base has `src/main.rs` — executable entry points belong in projects |
| `no-base` | A project has no base dependency — suppress with `[package.metadata.polylith] test-project = true` for test/dev projects |
| `not-in-workspace` | A component or base exists in its directory but is not listed in root workspace members |
| `ambiguous-interface` | Two or more components declare the same interface name but none has the default package name — every consumer must explicitly declare which implementation to use |
| `duplicate-name` | Two or more components share the same package name — rename the stub and declare `interface` metadata on both |
| `missing-interface` | Every component must declare `[package.metadata.polylith] interface = "..."` — use `component update <name>` or `cargo polylith edit` (press 'i') to set it |

Flags:
- `--json` — machine-readable output (`{"violations": [...]}`)

---

### `cargo polylith edit`

Interactive terminal UI for composing project dependencies.

```
cargo polylith edit
```

Displays a dependency grid — rows are components and bases, columns are projects. Each cell shows whether the brick is a direct dependency (`x`), transitive dependency (`·`), or not used (`-`).

**Transitive hover:** when the cursor rests on a transitive cell (`·`), the status bar shows the dependency chain that explains *why* the brick is pulled in — for example:

```
scaffold via: myproject → cli (base) → mcp → scaffold
```

Key bindings:

| Key | Action |
|---|---|
| `←→↑↓` / `hjkl` | Navigate |
| `Space` | Toggle direct dependency on/off |
| `i` | Edit the component's interface name (Enter to save, Esc to cancel) |
| `w` | Write changes to disk |
| `n` | Create a new project |
| `q` / `Esc` | Quit |

---

### `cargo polylith mcp serve`

Runs cargo-polylith as a [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server, exposing workspace analysis to AI assistants and other MCP clients.

```
cargo polylith mcp serve
cargo polylith mcp serve --write   # also enable scaffolding tools
```

Communicates over stdin/stdout using the standard MCP JSON-RPC transport.

**Read-only tools (always available):**

| Tool | Description |
|---|---|
| `polylith_info` | Workspace summary — components, bases, and projects |
| `polylith_deps` | Dependency graph, optionally filtered by brick name |
| `polylith_check` | Violations and warnings |
| `polylith_status` | Structural health summary |

**Write tools (enabled with `--write`):**

| Tool | Description |
|---|---|
| `polylith_component_new` | Create a new component crate |
| `polylith_base_new` | Create a new base crate |
| `polylith_project_new` | Create a new project workspace |
| `polylith_component_update` | Update a component's interface annotation |
| `polylith_set_implementation` | Select a component implementation for a project |

To wire up an AI assistant (e.g. Claude Code), add to `.mcp.json` at the workspace root:

```json
{
  "mcpServers": {
    "cargo-polylith": {
      "command": "cargo-polylith",
      "args": ["polylith", "mcp", "serve"]
    }
  }
}
```

---

## Global flags

All commands accept:

```
--workspace-root <PATH>
```

Override the workspace root instead of walking up from the current directory.
Useful when running from scripts or CI at arbitrary paths:

```bash
cargo polylith --workspace-root /path/to/repo info
cargo polylith --workspace-root /path/to/repo check --json
```

---

## Project layout

```
my-mono/
  Cargo.toml              ← root workspace (development project — all components + bases as members)
  .cargo/config.toml      ← shared target dir
  components/
    logger/
      Cargo.toml
      src/lib.rs
    payment/
      Cargo.toml
      src/lib.rs
    payment-stub/         ← lightweight stub for dev/test
      Cargo.toml
      src/lib.rs
  bases/
    rest-api/
      Cargo.toml
      src/lib.rs          ← exposes run()
  projects/
    production/
      Cargo.toml          ← standalone [workspace]; [dependencies] selects real components
    bdd/
      Cargo.toml          ← test/dev project; [dependencies] uses stubs
```

---

## Architecture

| Module | Role |
|---|---|
| `src/workspace/` | Read-only analysis: discover Cargo.toml files, build `WorkspaceMap`, run checks |
| `src/scaffold/` | Write-only: create directories and render file templates |
| `src/commands/` | Thin dispatch: parse CLI args → call workspace or scaffold → call output |
| `src/output/` | All terminal rendering and JSON serialisation |
