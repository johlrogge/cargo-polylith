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

## Next — polish and publish

- `cargo polylith check` hardening (more violation kinds, better messages)
- `cargo polylith edit` TUI composer (ratatui)
- Publish to crates.io once the MCP subcommand lands

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
