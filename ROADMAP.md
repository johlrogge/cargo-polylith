# Roadmap

## Shipped

### 0.14.1 — `polylith bump` refuses to over-bump dirty trees ✅

- `cargo polylith bump` now refuses to run in relaxed mode when `Polylith.toml`
  or root `Cargo.toml` (with `[workspace.package]`) has uncommitted modifications.
  Prevents the over-bump that occurred when a prior partial bump left the
  source-of-truth file already incremented; the next bump used to read that
  already-bumped value as "current" and increment again.
- New `--allow-dirty` flag (also exposed on the `polylith_bump` MCP tool) bypasses
  the check for cases where the user has intentionally edited the version file.
  Untracked files and non-git workspaces are unaffected.

### 0.14.0 — `.profile` files carry `[profile.*]` settings ✅

- `.profile` files now accept `[profile.release]`, `[profile.dev]`, `[profile.bench]`,
  and any `[profile.*]` sections alongside `[implementations]`. These pass through
  verbatim into the generated root `Cargo.toml` on `cargo polylith change-profile`,
  so profile-specific build settings (`strip`, `lto`, `opt-level`, nested package
  overrides) survive profile switches.
- `change-profile` and `migrate` warn on stderr when the current root `Cargo.toml`
  contains `[profile.*]` sections not declared in any `.profile`, so users upgrading
  from pre-0.14 workspaces can migrate those settings before the next regeneration
  overwrites them.

### 0.13.0 — `polylith_status` old-model profile hint ✅

- `polylith_status` now detects pre-0.11 profile directories (`profiles/<name>/Cargo.toml`)
  and emits a migration suggestion recommending `cargo polylith change-profile <name>`.
  Users upgrading from the old directory-per-profile layout get a clear, actionable hint
  rather than silent confusion.

### 0.12.0 — MCP `polylith_change_profile` write tool ✅

- `polylith_change_profile` added to the MCP write tool set (enabled with `--write`).
  Regenerates the root `Cargo.toml` from a named profile; `name` is a required argument.
  AI assistants can now switch profiles without leaving the MCP session.
- Internal: `[workspace.package]` reading moved from `scaffold/` into `workspace/`,
  enforcing the read-only / write-only module boundary.

### 0.9.0 — Profile workspaces generate root Cargo.toml ✅

Supersedes the symlink model (0.8.3). The root `Cargo.toml` IS the workspace; profiles
generate it directly. LSP/rust-analyzer works naturally because there are no subdirectory
workspaces or symlinks.

- `cargo polylith change-profile <name>` — writes a new root `Cargo.toml` generated from
  the named profile; old root `Cargo.toml` is committed or backed up before overwrite.
- `cargo polylith cargo --profile <name> <subcommand...>` — temporarily swaps the root
  `Cargo.toml` with a profile-generated one, runs cargo, then restores the original via
  a Drop guard (cleanup is guaranteed even on panic or error).
- `cargo polylith profile migrate` — generates root `Cargo.toml` from the dev profile
  instead of creating a symlinked subdirectory layout.
- Removed: symlinks, `profiles/<name>/` subdirectory workspaces, `profile build` command.

**Why:** the symlink model (0.8.3) broke editor integration — rust-analyzer and LSP clients
anchor to the root `Cargo.toml`, so a profile workspace under `profiles/dev/` was invisible
to the editor. Generating root `Cargo.toml` restores first-class editor support.

### 0.8.3 — Profile workspaces use symlinks (Option D) ✅ (superseded by 0.9.0)

Cargo 1.94+ requires workspace members to be hierarchically below the workspace root,
making `../../components/foo` member paths in `profiles/dev/Cargo.toml` invalid.

**Solution (Option D):** profile directories contained symlinks pointing back to the root
brick directories, and a generated `profiles/dev/Cargo.toml` used clean relative member
paths through those symlinks. This model is superseded by 0.9.0.

- `Polylith.toml` introduced as the workspace root marker (library versions, workspace.package metadata)

### 0.8.1 — `cargo polylith cargo` dev default, `profile migrate` ✅

- `cargo polylith cargo` now defaults `--profile` to `dev` when the flag is omitted. If no dev profile exists, prints: `no dev profile found — run 'cargo polylith profile migrate' to set one up`.
- `cargo polylith profile migrate [--force]` — migrates a workspace from the traditional "bricks in root workspace members" layout to the profiles-based model: reads `[workspace.dependencies]` interface path deps → writes `profiles/dev.profile` and regenerates the root `Cargo.toml` from the dev profile. `--force` overwrites an existing `profiles/dev.profile`. If the workspace is already migrated, exits cleanly with a message.

Post-migration workflow:
```
cargo polylith cargo check          # uses dev profile by default
cargo polylith cargo build
cargo polylith cargo test
cargo polylith cargo --profile production build
```

### 0.8.0 — Profile BFS transitive closure, `cargo polylith cargo` ✅

- `resolve_profile_workspace` now uses BFS transitive closure — only bricks transitively needed by the profile's selected implementations are included in the generated workspace. Alternative implementations of the same interface are excluded, enabling correct component-to-component swapping (e.g. a component that depends on `fact-store = { workspace = true }` — the profile controls which implementation `fact-store` resolves to).
- `cargo polylith cargo --profile <name> <subcommand...>` — generates the profile workspace and delegates to cargo against the root workspace. Accepts any cargo subcommand and trailing flags:
  ```
  cargo polylith cargo --profile production build
  cargo polylith cargo --profile dev test
  cargo polylith cargo --profile production clippy -- -D warnings
  ```
- `cargo polylith profile build <name>` is deprecated in favour of the new `cargo` subcommand.

### 0.7.0 — Base update, project set-impl, profile new, MCP expansion, ADRs ✅

Commands and tooling:

- `cargo polylith base update [--test-base]` — toggle a base between standard and test-base; sets or removes `test-base = true` in `[package.metadata.polylith]`
- `cargo polylith project set-impl <project> <interface> --implementation <name>` — write or update a component implementation selection in a project's `Cargo.toml`
- `cargo polylith profile new <name>` — create a new empty profile file at `profiles/<name>.profile`
- Name validation in `project new` and all MCP `_new` tools — rejects names with invalid characters before touching the filesystem
- New MCP write tools: `polylith_base_update`, `polylith_profile_new`, `polylith_profile_list`, `polylith_profile_add`
- 10 Architecture Decision Records added under `docs/adr/`

### 0.6.0 — Polylith profiles ✅

Named implementation sets that mirror the Clojure polylith profiles concept.

- `cargo polylith profile list [--json]` — lists all profiles and their selections
- `cargo polylith profile build <name> [--no-build]` — generates `profiles/<name>/Cargo.toml` (a standalone workspace applying the profile's overrides) and optionally runs `cargo build` (deprecated — use `cargo polylith cargo --profile <name> build` instead)
- `cargo polylith profile add <interface> --impl <path> --profile <name>` — adds or updates an implementation selection in a `.profile` file
- `cargo polylith check --profile <name>` — validates a named profile's implementation paths
- New check warnings: `hardwired-dep`, `profile-impl-not-found`, `profile-impl-not-component`

Profile files (`profiles/<name>.profile`) declare implementation overrides and extra library dependencies. `[workspace.dependencies]` in the root `Cargo.toml` is the wiring diagram; profiles override specific entries for different build targets.

### Earlier

MCP server (`cargo polylith mcp serve`) ✅ — read-only and write tools,
stdin/stdout JSON-RPC transport, wires directly into Claude Code and other MCP clients.

### Versioning model — relaxed mode ✅

Two-mode versioning configured in `Polylith.toml` (see [ADR-001](docs/adr/001-versioning-model.md)):

- `[versioning]` section added to `Polylith.toml` with `policy` (`relaxed` or `strict`) and `version` fields.
- `cargo polylith init` writes `policy = "relaxed"` and `version = "0.1.0"` by default.
- **`cargo polylith bump [major|minor|patch]`** — bumps the workspace version in `Polylith.toml` and the root `Cargo.toml` `[workspace.package]` version. Level argument required in relaxed mode.
- New `check` warning: `not-workspace-version` — brick not using `version.workspace = true` in a relaxed-mode workspace.
- MCP write tool: `polylith_bump` — exposes the same bump operation to agents.
- Generated `Cargo.toml` files now include a `# GENERATED BY cargo-polylith -- DO NOT EDIT` header.

### Versioning model — strict mode analysis ✅

- `policy = "strict"` in `Polylith.toml` activates strict mode.
- `tag_prefix` option in `[versioning]` controls the git-flow tag prefix used to locate the last release tag (default `v`).
- **`cargo polylith bump`** in strict mode (level is optional / auto-detected):
  - Finds the last git-flow release tag via `tag_prefix`
  - Compares public API surfaces of changed bricks using `syn`
  - Walks the dependency graph per project and accumulates change signals (API change → major, internal change → minor/patch, transitive-only → patch)
  - Reports a semver recommendation per project
- **`--dry-run`** flag — runs full analysis without writing any changes; useful in CI and pre-release review.
- Strict mode is currently analysis-only: project `Cargo.toml` versions are not written yet (planned next).
- `polylith_bump` MCP tool updated: `level` is now optional, `dry_run` accepted.

## Next

## Next — model alignment (legacy)

### ✅ Model correction: projects as bin crates, workspace as profile

The current model treats each project as a separate Cargo workspace under `projects/`.
This contradicts polylith's core principle of "one workspace, many deployable artifacts"
and loses all build caching between development and project builds.

**The corrected model:**
- A **project** is a crate with `[[bin]]` under `projects/`, listed as a member of the
  root workspace — not a workspace of its own.
- The **workspace** maps to the Clojure polylith concept of a **profile** — its `members`
  list determines what is in scope for compilation.
- Implementation selection stays in each project's `[dependencies]` via path deps with
  `package = "..."` aliasing — no `[patch.crates-io]` needed.
- A future "production" workspace (profile) would include only production bricks and
  their projects, enabling significantly faster CI builds.

**Why this matters:**
- Eliminates N+1 workspaces (one per project + root) → single unified workspace
- Shared build cache between `cargo build` (dev) and `cargo build -p my-project`
- Removes the `[patch.crates-io]` indirection from projects
- Aligns more faithfully with the Clojure polylith model

**Scope of work:**
1. Update `Project` struct — remove `members` and `patches` fields, add bin target info
2. Update `scan_projects` — detect bin crates as workspace members, not sub-workspaces
3. Update scaffold — `create project` produces a bin crate added to root workspace members
4. Update check rules — validate projects are workspace members, have at least one base dep
5. Update deps analysis — simplify now that everything lives in one workspace
6. Update VISION.md — document the profile mapping under "Rust-forced adaptations"

**Open question for Joakim Tengstrand:** In Clojure polylith, a profile can swap
implementations per context (dev vs prod). In this Cargo model, the workspace controls
scope (what is compiled) and each project's `[dependencies]` controls selection (which
implementation). Is this two-mechanism split a faithful mapping, or does it miss
something about how profiles are intended to work?

---

### ✅ TUI keybinding improvements (`cargo polylith edit`)

- `Esc` no longer quits — clears the status message instead (safe reset, Helix convention)
- `n` moved to `Ctrl-n` for new project — frees `n` from conflicting with Helix's next-search-match
- `gg` / `G` — jump to first/last row in the grid
- Dirty-quit guard — on first `q` with unsaved changes, warns "Unsaved changes — press w to save or q again to quit"; second `q` force-quits
- `i` on a base row now shows "Bases do not have interfaces" instead of silently doing nothing

---

### TUI: transitive dependency hover ✅
When the cursor rests on a cell marked as transitive, show the dependency chain
that explains *why* it is pulled in — e.g. `scaffold via: myproject → cli (base) → mcp → scaffold`.
Shown in the status bar.

### Docs pass with the documenter agent ✅
README.md and ROADMAP.md updated to reflect the current feature set (MCP server,
TUI edit, check hardening, polylith profiles).

### `cargo polylith check` hardening

#### Dep key / package name mismatch ✅
In `resolver = "2"` standalone project workspaces, a path dependency key must exactly
match the target's `package.name` — hyphens and underscores are NOT interchangeable.
A mismatch silently builds from the root workspace but fails in standalone project builds
with a confusing "no matching package found" error. Now detected as a hard violation
(`dep-key-mismatch`).

#### Other hardening
More violation kinds, clearer messages, better guidance text.

### Publish to crates.io
After model correction is implemented and validated with Joakim, and TUI is solid.
Requires coordination before cutting the release.

## Future

### Import crates.io library as a polylith component

Allow a component's implementation to be backed by a published crates.io crate rather
than local source. The component declares a named interface; the implementation is
`<crate-name> = { version = "...", package = "..." }` in the project's `[dependencies]`.
`cargo polylith check` would validate the mapping.

This is the inverse of "promoting a component to a standalone crate" — it lets teams
adopt external libraries within the polylith model without losing the named-interface
abstraction. A natural graduation path: prove a component's value locally, then swap in
a battle-tested crates.io crate behind the same interface.

### LSP server (`cargo polylith lsp serve`)

A Language Server Protocol server for Cargo.toml files, with polylith awareness.
No Cargo.toml LSP exists today; this would fill a genuine gap.

Capabilities:
- **Diagnostics** — `check` violations surfaced inline as you edit
- **Hover** — on a `path = "..."` dep, show interface name and alternative implementations
- **Completions** — interface names in `[package.metadata.polylith]`
- **Go-to-definition** — from `[patch.crates-io]` entries, jump to the substituted component
- **Code actions** — "add to workspace members", "set interface annotation"

Implementation shares `src/workspace/` with the CLI and MCP server.
Adds file-watching to keep the `WorkspaceMap` live as files change.
`tower-lsp` or `lsp-server` crate for the protocol layer.

Helix is the primary target (no existing Cargo.toml LSP support).

### Strict-mode bump — write per-project versions

The analysis phase of strict-mode `cargo polylith bump` is shipped. The remaining work:

- Write the recommended version to each affected project's `Cargo.toml` (currently analysis-only)
- Optionally update `void-packages/srcpkgs/*/template` `version=` fields for xbps-based deployment pipelines (`--apply` mode)
- Expose the per-project analysis as a `polylith_affected_projects` MCP tool, accepting an arbitrary git range (`since`..`until`) rather than requiring a release tag
