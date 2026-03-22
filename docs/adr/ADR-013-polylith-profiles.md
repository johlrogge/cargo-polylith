# ADR-013: Profiles as Named Implementation Sets

## Status
Accepted

## Context
Polylith workspaces may have components with multiple implementations (e.g. an in-memory store vs. a PostgreSQL store). Different build contexts (development, staging, production) require different implementation selections. Without a named grouping mechanism, each context would require manually specifying all implementation selections, making it easy to miss one and produce an inconsistent build.

## Decision
Profiles are named, reusable sets of implementation selections stored as TOML files in `profiles/<name>.profile`. A profile declares which implementation to use for each interface, and can be applied across projects. This mirrors the profile concept in Clojure polylith.

## Consequences
Switching between build contexts is a single profile selection rather than editing multiple project manifests. Profiles are version-controlled alongside the workspace. `cargo polylith check --profile <name>` validates a profile's selections against the current workspace state. Trade-off: profiles are a separate layer of indirection that must be kept in sync with available implementations as the workspace evolves.
