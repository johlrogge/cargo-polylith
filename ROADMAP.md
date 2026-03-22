# Roadmap

## Shipped

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
- `cargo polylith profile build <name> [--no-build]` — generates `profiles/<name>/Cargo.toml` (a standalone workspace applying the profile's overrides) and optionally runs `cargo build`
- `cargo polylith profile add <interface> --impl <path> --profile <name>` — adds or updates an implementation selection in a `.profile` file
- `cargo polylith check --profile <name>` — validates a named profile's implementation paths
- New check warnings: `hardwired-dep`, `profile-impl-not-found`, `profile-impl-not-component`

Profile files (`profiles/<name>.profile`) declare implementation overrides and extra library dependencies. `[workspace.dependencies]` in the root `Cargo.toml` is the wiring diagram; profiles override specific entries for different build targets.

### Earlier

MCP server (`cargo polylith mcp serve`) ✅ — read-only and write tools,
stdin/stdout JSON-RPC transport, wires directly into Claude Code and other MCP clients.

## Next — model alignment

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

### Check rule configuration
Support named rule sets so teams can define custom check rules, output formats,
or workspace conventions in `.polylith/config.toml` (or similar). Lets different
projects opt in to stricter or more lenient rule sets without forking the tool.
> Note: implementation-selection profiles (`cargo polylith profile`) shipped in 0.6.0.
> This item covers a separate concept: configurable check rules.

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

### `polylith_affected_projects` — affected project detection with semver suggestions

Given a git range (`since`..`until`), determines which projects need a version bump
and what bump level (patch/minor/major) is warranted based on conventional commits.

**MCP tool parameters:**
- `path` (required) — workspace root (must be a git repo)
- `since` (required) — git ref to compare from (tag, commit, branch)
- `until` (optional, default `HEAD`)

**Algorithm:**
1. `git diff --name-only <since>..<until>` → changed files
2. Map files to components by path prefix (`components/foo_bar/` → `foo-bar`)
3. For each project: resolve full transitive component dep graph
4. A project is affected if any transitive dep changed, OR its own `src/` changed
5. Determine semver bump level from conventional commit messages in range:
   - `fix:` → patch; `feat:` → minor; `feat!:` / `BREAKING CHANGE:` → major
   - Mixed: take the highest level across all commits touching affected components

**Output:** table of affected projects with current → suggested version and reason.

**`--apply` mode:** Write suggested versions to each affected project's `Cargo.toml`
and update matching `void-packages/srcpkgs/*/template` `version=` fields to stay in
sync with xbps-based deployment pipelines.

**Why high value:** Polylith already owns the dep graph — adding git-range + conventional
commits makes it a complete release oracle. `--apply` makes version bumping a one-command
operation across N projects.
