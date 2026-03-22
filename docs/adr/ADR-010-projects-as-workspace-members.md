# ADR-010: Projects as Root Workspace Members (Model Correction)

## Status
Accepted

## Context
An earlier model placed each polylith project in its own independent Cargo sub-workspace under `projects/`. This required `[patch.crates-io]` to make interface substitution work across workspace boundaries, and created N+1 separate build graphs (one per project plus the root). This fragmented the build cache and made cross-project compilation slow.

## Decision
Projects are bin crates listed directly in the root workspace's `[workspace].members`. A project is a `src/main.rs` crate with path dependencies on its chosen component implementations, compiled as part of the single unified workspace.

## Consequences
One workspace, one build cache, multiple deployable artifacts — faithful to Polylith's core principle. `[patch.crates-io]` is no longer needed. Running `cargo build -p my-project` shares compilation output with regular `cargo build`. The check `ProjectNotInRootWorkspace` enforces this invariant and catches projects that were placed incorrectly as sub-workspaces.
