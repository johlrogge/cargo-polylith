# ADR-001: Strict Read/Write Module Boundaries

## Status
Accepted

## Context
CLI tools often mix analysis and mutation logic, making them hard to test and extend. cargo-polylith needs to support multiple output interfaces (CLI, MCP server, TUI). Without explicit boundaries, workspace analysis code becomes entangled with file-writing and rendering concerns, making it impossible to reuse across interfaces without dragging in unrelated dependencies.

## Decision
Enforce strict separation: `src/workspace/` is read-only analysis only (discovers Cargo.toml files, builds WorkspaceMap, runs checks — never writes). `src/scaffold/` is write-only (creates dirs and template files — no parsing). `src/commands/` is thin dispatch. `src/output/` handles all terminal rendering.

## Consequences
Workspace logic is reusable by MCP server, TUI, and future LSP without modification. Testing workspace analysis requires no file I/O mocking. Adding a new output interface doesn't touch core logic. The constraint is enforced by convention, not the type system.
