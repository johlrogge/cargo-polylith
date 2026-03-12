# cargo-polylith

A Cargo subcommand that brings the [polylith](https://polylith.gitbook.io/polylith/) monorepo
architecture model to Rust/Cargo workspaces.

Polylith organises code into three kinds of brick:

- **Components** — library crates with a stable public interface (`pub use <name>::*`)
- **Bases** — binary crates that wire components together into runnable programs
- **Projects** — thin workspace manifests that select a set of bases (and their transitive
  component deps) for a deployable artefact

All analysis is pure TOML + filesystem — no `cargo metadata` invocation, so the tool works
even when the workspace doesn't fully compile.

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
# → components/logger/src/lib.rs   (pub use logger::*)
# → components/logger/src/logger.rs

# 4. Add a base (binary)
cargo polylith base new api
# → bases/api/Cargo.toml  (with [[bin]])
# → bases/api/src/main.rs

# 5. Create a deployable project
cargo polylith project new production
# → projects/production/Cargo.toml  (workspace + [patch] placeholders)

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
- `bases/` — home for base (binary) crates
- `projects/` — home for project workspace manifests
- `.cargo/config.toml` — sets shared `target/` dir

Warns (but does not fail) if any of those directories already exist.

---

### `cargo polylith component new <name>`

Creates a new component crate.

```
cargo polylith component new payment
```

Produces:

```
components/payment/
  Cargo.toml
  src/
    lib.rs        ← mod payment; pub use payment::*;
    payment.rs    ← implementation stub
```

Also appends `"components/payment"` to `[workspace].members` in the root `Cargo.toml`
using `toml_edit` (preserves existing comments and formatting).

---

### `cargo polylith base new <name>`

Creates a new base (binary) crate.

```
cargo polylith base new rest-api
```

Produces:

```
bases/rest-api/
  Cargo.toml     ← [[bin]] section pointing at src/main.rs
  src/
    main.rs
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
  Cargo.toml     ← [workspace] with member/patch placeholders
```

Edit the manifest to add the bases you want to ship and use `[patch]` to point
component dependencies at your local implementations.

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

| Kind | Description |
|---|---|
| `missing_lib_rs` | A component has no `src/lib.rs` |
| `missing_re_export` | `lib.rs` is missing `pub use <name>::*` |
| `missing_impl_file` | A component is missing `src/<name>.rs` |
| `missing_main_rs` | A base has no `src/main.rs` |
| `base_dep_on_base` | A base lists another base as a dependency |

**Warnings** (exit 0):

| Kind | Description |
|---|---|
| `orphan_component` | A component is not used by any base |

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
  Cargo.toml              ← root workspace (lists all components + bases as members)
  .cargo/config.toml      ← shared target dir
  components/
    logger/
      Cargo.toml
      src/lib.rs
      src/logger.rs
    payment/
      Cargo.toml
      src/lib.rs
      src/payment.rs
  bases/
    rest-api/
      Cargo.toml
      src/main.rs
  projects/
    production/
      Cargo.toml          ← its own [workspace] selecting bases + [patch] for components
```

---

## Architecture

| Module | Role |
|---|---|
| `src/workspace/` | Read-only analysis: discover Cargo.toml files, build `WorkspaceMap`, run checks |
| `src/scaffold/` | Write-only: create directories and render file templates |
| `src/commands/` | Thin dispatch: parse CLI args → call workspace or scaffold → call output |
| `src/output/` | All terminal rendering and JSON serialisation |
