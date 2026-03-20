# Roadmap

## Now — MCP server (`cargo polylith mcp serve`)

Expose workspace analysis over the Model Context Protocol as a built-in subcommand.
Shares `src/workspace/` directly — no separate project, no drift.

Tools to expose:
- `polylith_info` — workspace summary (components, bases, projects)
- `polylith_deps` — dependency graph, optionally filtered by brick
- `polylith_check` — violations and warnings
- `polylith_status` — structural health summary

Invoked as `cargo polylith mcp serve` (stdin/stdout, standard MCP transport).
`.mcp.json` points at the installed binary — no separate server to maintain.

## Next — TUI polish and model alignment

### ⚠️ Model correction: projects as bin crates, workspace as profile [HIGHEST PRIORITY]

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

### TUI: transitive dependency hover ✅
When the cursor rests on a cell marked as transitive, show the dependency chain
that explains *why* it is pulled in — e.g. `scaffold via: myproject → cli (base) → mcp → scaffold`.
Shown in the status bar.

### Profiles (configuration)
Support named configuration profiles so teams can define custom check rules,
output formats, or workspace conventions in `.polylith/profiles.toml` (or similar).
Profiles let different projects opt in to stricter or more lenient rule sets without
forking the tool.
> Note: depends on model correction above to avoid naming confusion with workspace profiles.

### Docs pass with the documenter agent
Run the `documenter` agent over README.md, ROADMAP.md, and any generated docs to
ensure they reflect the current feature set (MCP server, TUI edit, check hardening).

### `cargo polylith check` hardening
More violation kinds, clearer messages, better guidance text.

### Publish to crates.io
After model correction is implemented and validated with Joakim, and TUI is solid.
Requires coordination before cutting the release.

## Future — LSP server (`cargo polylith lsp serve`)

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
