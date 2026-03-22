# ADR-003: Explicit Interface Metadata in Cargo.toml

## Status
Accepted

## Context
Clojure polylith identifies component interfaces via namespace conventions (`com.myorg.interface`). Rust has no equivalent namespace-based interface concept — a crate name doesn't encode its interface identity. Without an explicit declaration mechanism, the tool would have to infer interface membership from naming conventions, which is fragile and ambiguous when multiple components implement the same interface.

## Decision
Components declare their interface name explicitly in `[package.metadata.polylith] interface = "..."` in their Cargo.toml. The tool reads this field to build the component/interface map.

## Consequences
Interface membership is unambiguous and typo-resistant (a misspelled name is caught, not silently creating a phantom interface). The metadata is visible and editable without running any tool commands. Trade-off: scaffolded components require this field to be meaningful; the tool emits a `missing-interface` warning when it's absent.
