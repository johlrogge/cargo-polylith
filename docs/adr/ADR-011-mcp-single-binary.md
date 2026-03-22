# ADR-011: MCP Server Shares Workspace Module Inside Single Binary

## Status
Accepted

## Context
To support AI assistants (e.g. Claude Code) querying polylith workspace structure, an MCP (Model Context Protocol) server is needed. This could be a separate binary or library crate. cargo-polylith itself is a single-crate project with no internal path dependencies. A separate binary would require separate installation and separate maintenance of the workspace analysis logic.

## Decision
The MCP server is implemented as a subcommand (`cargo polylith mcp serve`) within the same binary. It reuses `src/workspace/` directly, exposing the same analysis over JSON-RPC stdin/stdout without any additional abstraction layer.

## Consequences
No separate deployment step — installing `cargo-polylith` provides the MCP server. The workspace analysis logic has one canonical implementation used by CLI, TUI, and MCP. Trade-off: the binary is larger; the MCP surface is only as capable as the workspace module exposes. Any improvement to workspace analysis is immediately available to MCP clients without a separate release.
