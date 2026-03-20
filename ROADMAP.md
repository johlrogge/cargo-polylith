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

### TUI: transitive dependency hover
When the cursor rests on a cell marked as transitive, show the dependency chain
that explains *why* it is pulled in — e.g. `project → cli (base) → mcp → scaffold`.
Surfaced as a status-bar message or inline popup using ratatui.

### Profiles
Support named configuration profiles so teams can define custom check rules,
output formats, or workspace conventions in `.polylith/profiles.toml` (or similar).
Profiles let different projects opt in to stricter or more lenient rule sets without
forking the tool.

### Model alignment review with Joakim Tengstrand (Clojure polylith)
Walk through the Clojure polylith model side-by-side and document where the Rust
implementation intentionally diverges (e.g. bases may depend on bases, path deps
instead of `[patch]`) and where gaps exist that should be closed. Outcome: a clear
"intentional differences" document and a backlog of model gaps to fix.

### Docs pass with the documenter agent
Run the `documenter` agent over README.md, ROADMAP.md, and any generated docs to
ensure they reflect the current feature set (MCP server, TUI edit, check hardening).

### `cargo polylith check` hardening
More violation kinds, clearer messages, better guidance text.

### Publish to crates.io
After model alignment sign-off with Joakim and TUI is solid.
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
