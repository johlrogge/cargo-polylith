# ADR-008: Path Dependencies with `package =` Aliasing as the Swap Mechanism

## Status
Accepted

## Context
Polylith's core concept is that a project selects which implementation of an interface to compile. In Cargo, this requires some mechanism to substitute one crate for another under the same name. An earlier approach used `[patch.crates-io]` in the root workspace to redirect interface names to implementation crates, but this created hidden indirection and coupled all projects to a single workspace-level selection.

## Decision
Replace `[patch.crates-io]` with direct path dependencies using `package = "..."` aliasing in each project's `[dependencies]`. A project selects an implementation by declaring: `interface-name = { path = "../components/impl-name", package = "impl-name" }`.

## Consequences
The selection is explicit and local to the project's own manifest — no hidden indirection through workspace-level patch tables. Each project's dependency block is self-describing. Trade-off: each project must explicitly list all its implementation selections rather than inheriting workspace-level patches. This verbosity is intentional: it makes the build configuration of each deployable artifact fully readable from its own Cargo.toml.
