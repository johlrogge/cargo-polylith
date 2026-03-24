# ADR-015: `Polylith.toml` Is the Canonical Workspace Root Marker

## Status
Accepted

## Context
Previously, the presence of `[workspace]` in the root `Cargo.toml` served as the indicator that a directory was a polylith workspace root. This conflicts with the profile workspace mechanism (ADR-014): Cargo's workspace walk-up causes every package under the repository root to be claimed by the root workspace, preventing any profile workspace from claiming them as its own members. A separate marker that does not interact with Cargo's workspace resolution is required.

## Decision
`Polylith.toml` at the project root is the canonical marker for a polylith workspace. `find_workspace_root` searches for this file and prefers it over `Cargo.toml [workspace]`. After `cargo polylith profile migrate`, the root `Cargo.toml` retains only a placeholder `[package]` section with no `[workspace]`; polylith-level metadata — library versions, workspace.package defaults, and the profile registry — moves into `Polylith.toml`. A plain `.polylith` marker file was considered but rejected as less discoverable and unable to carry structured data.

## Consequences
The root `Cargo.toml` is no longer the source of truth for workspace membership; that role belongs to each profile workspace. `Polylith.toml` becomes a required file in any workspace managed by `cargo-polylith`. Tooling that previously detected polylith workspaces by inspecting `Cargo.toml` must be updated to look for `Polylith.toml`. The separation also opens a clean location for future workspace-level configuration without polluting `Cargo.toml`.
