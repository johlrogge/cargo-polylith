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

**Projects** — bin crates under `projects/`, listed as members of a profile workspace. Each project
selects a specific combination of bases and component implementations via path dependencies in
`[dependencies]`. Projects must not contain a `[workspace]` section of their own.

**Development project** — the root `Cargo.toml`, generated from the dev profile by
`cargo polylith profile migrate` (or `cargo polylith change-profile dev`). It includes all
components, bases, and projects as members and is optimised for fast feedback during development
and testing. Components default to lightweight implementations (stubs, in-memory versions);
profiles select production-grade implementations as needed.

**Interfaces** — named contracts declared in `[package.metadata.polylith]`. Two components with
the same interface name are alternative implementations. The Rust compiler enforces type
compatibility when an implementation is swapped; `cargo polylith check` performs a structural
pre-check on public symbol names.

---

## Adaptations from the Clojure reference implementation

| Concept | Clojure polylith | cargo-polylith | Why |
|---|---|---|---|
| **Interface declaration** | Namespace structure — components implement an interface by having the same namespace | `[package.metadata.polylith] interface = "..."` in Cargo.toml | Rust has no namespace-based interface; explicit metadata is unambiguous and prevents typos from creating phantom interfaces |
| **Profile / implementation switching** | Named profiles in `deps.edn` select which source directories are compiled in | Named profiles stored in `profiles/<name>.profile`; `cargo polylith change-profile <name>` generates the root `Cargo.toml` from a profile (making it the active workspace); `cargo polylith cargo --profile <name> <subcommand>` temporarily swaps the root `Cargo.toml`, runs cargo, then restores the original. Swapping works at the brick level — one component can depend on another via a named interface, and the profile selects which implementing brick is compiled in. | Mirrors Clojure polylith's profile concept — `[workspace.dependencies]` is the wiring diagram, profiles override specific entries for different build targets |
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
# → projects/production/Cargo.toml  (bin crate, added to root workspace members)
# → projects/production/src/main.rs

# 6. Inspect the workspace
cargo polylith info
cargo polylith deps

# 7. Validate structure
cargo polylith check

# 8. Migrate to the profile-based build system
cargo polylith profile migrate
# → writes profiles/dev.profile and regenerates root Cargo.toml from the dev profile
# → root Cargo.toml IS the active workspace; LSP/rust-analyzer work naturally

# 9. Build normally (root Cargo.toml is the dev workspace)
cargo build
cargo test

# 10. Work with additional profiles (optional — for named implementation sets)
cargo polylith profile add http-client --impl components/http-client-real --profile production
cargo polylith profile list
cargo polylith change-profile production          # overwrites root Cargo.toml with production profile
cargo polylith cargo --profile production build   # temporarily swaps root Cargo.toml, then restores
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

### `cargo polylith base update <name>`

Toggles a base between a standard base and a test-base.

```
cargo polylith base update rest-api
cargo polylith base update rest-api --test-base
```

`--test-base` sets `[package.metadata.polylith] test-base = true` in the base's `Cargo.toml`,
which suppresses the `no-base` warning for projects that depend only on this base.
Omitting the flag removes the annotation (or keeps it absent), reverting to a standard base.

---

### `cargo polylith project new <name>`

Creates a project workspace manifest.

```
cargo polylith project new production
```

Produces:

```
projects/production/
  Cargo.toml     ← bin crate; also added to root workspace members
  src/
    main.rs      ← thin entry point; calls a base's run()
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
| `project-has-own-workspace` | A project `Cargo.toml` contains a `[workspace]` section — projects must be bin crates in the root workspace, not sub-workspaces |
| `project-not-in-root-workspace` | A project under `projects/` is not listed as a member of the root workspace `[workspace].members` |

**Warnings** (exit 0):

| Tag | Description |
|---|---|
| `orphan` | Component is not reachable from any base or project (including as a swapped implementation via `package =`) |
| `wildcard` | Component's `lib.rs` uses `pub use <crate>::*` — prefer explicit re-exports |
| `base-has-main` | A base has `src/main.rs` — executable entry points belong in projects |
| `no-base` | A project has no base dependency — polylith projects must include at least one base |
| `not-in-workspace` | A component or base exists in its directory but is not listed in root workspace members |
| `ambiguous-interface` | Two or more components declare the same interface name but none has the default package name — every consumer must explicitly declare which implementation to use |
| `duplicate-name` | Two or more components share the same package name — rename the stub and declare `interface` metadata on both |
| `missing-interface` | Every component must declare `[package.metadata.polylith] interface = "..."` — use `component update <name>` or `cargo polylith edit` (press 'i') to set it |
| `hardwired-dep` | Component or base has a direct path dependency to another workspace component instead of `{ workspace = true }` — bypasses the wiring diagram |
| `profile-impl-not-found` | A profile references a component path that does not exist in the workspace |
| `profile-impl-not-component` | A profile references a path that is not a known workspace component |
| `not-workspace-version` | In relaxed versioning mode, a brick's `Cargo.toml` does not use `version.workspace = true` — all brick versions should follow the workspace version |

Flags:
- `--json` — machine-readable output (`{"violations": [...]}`)
- `--profile <name>` — validate a named profile's implementation paths in addition to workspace structure

---

### `cargo polylith profile`

Manages named sets of implementation selections that can be applied workspace-wide.
Profiles mirror the Clojure polylith concept: `[workspace.dependencies]` in the root
`Cargo.toml` is the "wiring diagram"; profiles override specific entries to select
different implementations for different build targets (production vs dev stubs, etc.).

Profile files live at `profiles/<name>.profile` and use the following format:

```toml
[implementations]
http-client  = "components/http-client-hato"
email-sender = "components/email-sender-smtp"

[libraries]
tokio = { version = "1", features = ["rt-multi-thread"] }
```

#### `cargo polylith profile new <name>`

Creates a new empty profile file.

```
cargo polylith profile new staging
```

Creates `profiles/staging.profile` with an empty `[implementations]` section.
Use `profile add` to populate it.

#### `cargo polylith profile list`

Lists all profiles and their implementation selections.

```
cargo polylith profile list
cargo polylith profile list --json
```

Flags:
- `--json` — machine-readable output

#### `cargo polylith profile add <interface> --impl <path> --profile <name>`

Adds or updates an implementation selection in a `.profile` file.

```
cargo polylith profile add http-client \
  --impl components/http-client-hato \
  --profile production
```

Creates `profiles/<name>.profile` if it does not exist.

#### `cargo polylith profile migrate [--force]`

Migrates a workspace from the traditional "bricks in root workspace members" layout
to the profiles-based model.

```
cargo polylith profile migrate
cargo polylith profile migrate --force
```

What it does:

1. Reads `[workspace.dependencies]` interface path deps from the root `Cargo.toml`
2. Writes `profiles/dev.profile` with those selections under `[implementations]`
3. Regenerates the root `Cargo.toml` from the dev profile — the root workspace IS the
   active dev workspace; no subdirectory workspaces or symlinks are created
4. Strips `{ workspace = true }` from brick `Cargo.toml`s so they are self-contained

If the workspace is already migrated, exits cleanly with a message and makes no changes.
`--force` overwrites an existing `profiles/dev.profile`.

After migration, run `cargo` directly as normal — the root `Cargo.toml` is the dev
workspace. Use `cargo polylith cargo --profile <name>` to build under a different profile
without permanently switching:

```
cargo build                                       # dev workspace (root Cargo.toml)
cargo test
cargo polylith cargo --profile production build   # temporarily swaps root Cargo.toml, then restores
cargo polylith change-profile production          # permanently write production profile to root Cargo.toml
```

---

### `cargo polylith change-profile <name>`

Generates the root `Cargo.toml` from the named profile and writes it in place. The root
workspace becomes the active profile workspace; LSP/rust-analyzer pick it up naturally
because they anchor to the root `Cargo.toml`.

```
cargo polylith change-profile dev
cargo polylith change-profile production
```

The previous root `Cargo.toml` is backed up before it is overwritten.

After switching, run `cargo` directly — no wrapper needed:

```
cargo build
cargo test
cargo clippy
```

To switch back:

```
cargo polylith change-profile dev
```

---

### `cargo polylith cargo [--profile <name>] <subcommand...>`

Temporarily swaps the root `Cargo.toml` with one generated from the named profile,
delegates to cargo, then restores the original. Cleanup is guaranteed via a Drop guard,
even on panic or error. Accepts any cargo subcommand and trailing flags.

`--profile` defaults to `dev` when omitted. If no dev profile exists, the command
prints `no dev profile found — run 'cargo polylith profile migrate' to set one up`.

```
cargo polylith cargo check                          # uses dev profile by default
cargo polylith cargo build
cargo polylith cargo test
cargo polylith cargo --profile production build
cargo polylith cargo --profile production clippy -- -D warnings
```

For day-to-day development, run `cargo` directly — the root `Cargo.toml` is the active
workspace after `profile migrate` or `change-profile`. Use `cargo polylith cargo` when
you want to invoke cargo under a different profile without permanently switching.

Only the bricks transitively needed by the profile's selected implementations are
included in the generated manifest — alternative implementations of the same interface
are excluded. This enables correct component-to-component swapping: if a component
depends on `fact-store = { workspace = true }`, the profile controls which implementation
`fact-store` resolves to.

---

### `cargo polylith bump [level] [--dry-run]`

Bumps the workspace version. Behaviour depends on the versioning policy in `Polylith.toml`.

**Relaxed mode** — level is required:

```
cargo polylith bump patch    # 0.3.1 → 0.3.2
cargo polylith bump minor    # 0.3.1 → 0.4.0
cargo polylith bump major    # 0.3.1 → 1.0.0
```

Writes the new version to `Polylith.toml` and (if `[workspace.package]` is present) to the root `Cargo.toml`.

**Strict mode** — level is auto-detected:

```
cargo polylith bump            # analyzes changes, recommends per-project bump levels
cargo polylith bump --dry-run  # same analysis, no writes
```

`cargo polylith bump` in strict mode:

1. Finds the last git-flow release tag (respecting `tag_prefix` in `[versioning]`)
2. For each brick changed since that tag, compares the public API surface using `syn`
3. Walks the dependency graph per project and accumulates change signals:
   - Public API change → major signal
   - Internal change only → minor or patch (informed by conventional commits)
   - Transitive dependency change only → patch signal
4. Reports a semver recommendation per project
5. Writes the new workspace version to `Polylith.toml` (skipped with `--dry-run`)

Strict mode is currently analysis-only — project `Cargo.toml` versions are not written yet. `--dry-run` suppresses the workspace version write as well, making the command a pure report.

See [Versioning](#versioning) below for how to configure `Polylith.toml`.

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
| `i` | Edit the component's interface name (Enter to save, Esc to cancel); shows "Bases do not have interfaces" on base rows |
| `w` | Write changes to disk |
| `Ctrl-n` | Create a new project |
| `Esc` | Clear status message |
| `q` | Quit (warns on first press if there are unsaved changes; press `q` again to force-quit) |
| `gg` / `G` | Jump to first / last row |

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
| `polylith_profile_list` | List all profiles and their implementation selections |

**Write tools (enabled with `--write`):**

| Tool | Description |
|---|---|
| `polylith_component_new` | Create a new component crate |
| `polylith_base_new` | Create a new base crate |
| `polylith_base_update` | Toggle a base between standard and test-base |
| `polylith_project_new` | Create a new project workspace |
| `polylith_component_update` | Update a component's interface annotation |
| `polylith_profile_new` | Create a new empty profile file |
| `polylith_profile_add` | Add or update an implementation selection in a profile |
| `polylith_bump` | Bump the workspace version in `Polylith.toml`; `level` (`major`, `minor`, `patch`) required in relaxed mode, auto-detected in strict mode; accepts `dry_run: true` |
| `polylith_migrate_package_meta` | Overwrite root `Cargo.toml [package]` fields with values from `Polylith.toml [workspace.package]`, then remove `[workspace.package]` from `Polylith.toml`. Fields present in `Polylith.toml` overwrite existing values in `Cargo.toml`; fields absent from `Polylith.toml` are left untouched. |

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

## Versioning

`Polylith.toml` carries the workspace version and versioning policy. `cargo polylith init` creates `Polylith.toml` with relaxed mode enabled by default:

```toml
[workspace]
schema_version = 1

[versioning]
policy = "relaxed"
version = "0.1.0"
```

### Relaxed mode (default)

All brick versions equal the workspace version. Bricks declare `version.workspace = true` in their `Cargo.toml` and follow the workspace version automatically. Use `cargo polylith bump <level>` to advance the version at release time.

`cargo polylith check` warns (`not-workspace-version`) for any brick whose `Cargo.toml` does not use `version.workspace = true`.

### Strict mode

Each brick owns its version as a change-tracking signal, bumped during development. At release time, `cargo polylith bump` (no level argument required) analyzes the public API surface of changed bricks with `syn`, walks the dependency graph, and recommends a semver bump level per project.

Enable strict mode in `Polylith.toml`:

```toml
[versioning]
policy = "strict"
version = "0.1.0"
tag_prefix = "v"   # optional; matches your git-flow tag prefix (default "v")
```

`tag_prefix` controls how the tool identifies the last release tag when comparing brick versions. It should match what `git flow init` configured (typically `v`).

See [ADR-001](docs/adr/001-versioning-model.md) for the full design rationale.

### Generated files

Project `Cargo.toml` files generated by `cargo-polylith` include a header comment:

```toml
# GENERATED BY cargo-polylith -- DO NOT EDIT
```

Component and base `Cargo.toml` files are user-owned and are never overwritten.

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
  Cargo.toml              ← root manifest; generated from the active profile (dev by default)
  .cargo/config.toml      ← shared target dir
  Polylith.toml           ← workspace version and versioning policy
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
      Cargo.toml          ← bin crate; [dependencies] selects real components
      src/main.rs
    bdd/
      Cargo.toml          ← test/dev project; [dependencies] uses stubs
      src/main.rs
  profiles/
    dev.profile           ← implementation selections for dev/test (default)
    production.profile    ← implementation selections for production builds
    staging.profile       ← implementation selections for staging builds
```

---

## Architecture

| Module | Role |
|---|---|
| `src/workspace/` | Read-only analysis: discover Cargo.toml files, build `WorkspaceMap`, run checks |
| `src/scaffold/` | Write-only: create directories and render file templates |
| `src/commands/` | Thin dispatch: parse CLI args → call workspace or scaffold → call output |
| `src/output/` | All terminal rendering and JSON serialisation |

---

## Releasing

See [RELEASING.md](RELEASING.md) for the full release checklist (git-flow branch model, pre-release steps, versioning rules).
