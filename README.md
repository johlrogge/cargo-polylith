# cargo-polylith

A Cargo subcommand that brings the [polylith](https://polylith.gitbook.io/polylith/) monorepo
architecture model to Rust/Cargo workspaces.

All analysis is pure TOML + filesystem — no `cargo metadata` invocation, so the tool works
even when the workspace doesn't fully compile.

---

## The polylith model — our Rust interpretation

Polylith organises code into four building blocks:

**Components** — library crates that implement a named interface. Each component declares its
interface via `[package.metadata.polylith] interface = "<name>"` in its `Cargo.toml`. Multiple
components may implement the same interface name; exactly one is active in any given build
context (selected via `[patch.crates-io]` in a project workspace).

**Bases** — entry-point crates that wire components into a runnable program. Bases depend on
components and expose a library API (`src/lib.rs`) so that project workspace manifests can call
their `run()` function from a thin `src/main.rs`. Bases must not have their own `src/main.rs`.

**Projects** — standalone Cargo workspaces in `projects/`. Each project is an independently
buildable workspace that selects a specific combination of bases and, via `[patch.crates-io]`,
chooses which component implementations to use.

**Development project** — the root workspace itself. It contains all components and bases and
is optimised for fast feedback during development and testing. Components in the root workspace
default to lightweight implementations (stubs, in-memory versions). Projects patch in
production-grade implementations as needed.

**Interfaces** — named contracts declared in `[package.metadata.polylith]`. Two components with
the same interface name are alternative implementations. The Rust compiler enforces type
compatibility when an implementation is swapped via `[patch]`; `cargo polylith check` performs a
structural pre-check on public symbol names.

---

## Adaptations from the Clojure reference implementation

| Concept | Clojure polylith | cargo-polylith | Why |
|---|---|---|---|
| **Interface declaration** | Namespace structure — components implement an interface by having the same namespace | `[package.metadata.polylith] interface = "..."` in Cargo.toml | Rust has no namespace-based interface; explicit metadata is unambiguous and prevents typos from creating phantom interfaces |
| **Profile / implementation switching** | Named profiles in `deps.edn` select which source directories are compiled in | `[patch.crates-io]` in a project workspace Cargo.toml | Cargo `[patch]` is the closest analog — compile-time substitution of one crate for another |
| **Development project** | A dedicated `development/` project at the workspace root | The root workspace itself | Cargo's workspace model is already the right structure; no separate project needed |
| **Stub-first development** | Default profile uses the primary implementation | Root workspace uses lightweight/stub components by default; production projects patch in the real thing | Enables fast tests without heavy deps (PipeWire, file scanning, sha2, etc.) |
| **Test/dev projects** | The development project has no base requirement | Projects in `projects/` that are test or development harnesses do not require a base dependency | A test runner is an entry point in its own right; forcing a base dependency would be artificial |
| **Interface compatibility** | `poly` tool checks that components implementing the same interface have matching public APIs | Rust compiler enforces compatibility when you swap via `[patch]`; `cargo polylith check` does a structural pre-check on public symbol names | Rust's type system is more expressive than namespace-based interfaces — let the compiler do the definitive check |
| **One interface, two implementations without a default** | Both implementations live in the workspace; profiles select | Both components have the same interface name but neither has a package name matching the interface — every consumer must `[patch]` explicitly | Makes the choice intentional rather than implicit; the tool warns `AmbiguousInterface` |

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
# → projects/production/Cargo.toml  (standalone workspace + [patch] placeholders)

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
  Cargo.toml     ← standalone [workspace] with member/patch placeholders
```

Edit the manifest to add the bases you want to ship and use `[patch.crates-io]` to swap in
the component implementations appropriate for that project:

```toml
[dependencies]
# Declare the interface needed — implementation chosen below.
library-service = "0.1"

[patch.crates-io]
# Swap the stub for the real component in production.
library-service = { path = "../../components/library_service_real",
                    package = "library-service-real" }
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
| `base-dep-base` | A base depends on another base |

**Warnings** (exit 0):

| Tag | Description |
|---|---|
| `orphan` | Component is not reachable from any base or project (including via `[patch]` substitution) |
| `wildcard` | Component's `lib.rs` uses `pub use <crate>::*` — prefer explicit re-exports |
| `base-has-main` | A base has `src/main.rs` — executable entry points belong in projects |
| `no-base` | A project has no base dependency — suppress with `[package.metadata.polylith] test-project = true` for test/dev projects |
| `not-in-workspace` | A component or base exists in its directory but is not listed in root workspace members |
| `ambiguous-interface` | Two or more components declare the same interface name but none has the default package name — every consumer must `[patch]` explicitly |
| `duplicate-name` | Two or more components share the same package name — rename the stub and declare `interface` metadata on both |
| `missing-interface` | Every component must declare `[package.metadata.polylith] interface = "..."` — use `component update <name>` or `cargo polylith edit` (press 'i') to set it |

Flags:
- `--json` — machine-readable output (`{"violations": [...]}`)

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
      Cargo.toml          ← standalone [workspace]; [patch.crates-io] selects real components
    bdd/
      Cargo.toml          ← test/dev project; patches in stubs
```

---

## Architecture

| Module | Role |
|---|---|
| `src/workspace/` | Read-only analysis: discover Cargo.toml files, build `WorkspaceMap`, run checks |
| `src/scaffold/` | Write-only: create directories and render file templates |
| `src/commands/` | Thin dispatch: parse CLI args → call workspace or scaffold → call output |
| `src/output/` | All terminal rendering and JSON serialisation |
