# ADR-016: Corsett Label Shortening for TUI Column Headers

## Status
Accepted

## Context
The `edit` TUI displays a grid where each column represents a project. Large polylith workspaces have many projects with long, often similarly prefixed names (e.g. `myapp-api-staging`, `myapp-api-production`). Rendering full names overflows the available terminal width, making the grid unusable. Truncating naively produces collisions; hiding columns loses information entirely.

## Decision
Column headers use `crate::corsett::fit_group` to compress project names to fit within the available terminal width. The algorithm compacts common leading path segments first and guarantees that all shortened forms within a group remain mutually unique. The shortened labels are used only for display; the underlying project identity is unchanged.

## Consequences
All projects remain visible in the TUI regardless of terminal width, and users can distinguish projects by their shortened labels. The shortening is deterministic, so the same workspace always produces the same column headers for a given terminal width. The approach adds a dependency on the `corsett` module for display logic; if project names are so numerous or so similar that no unique shortening exists within the available width, the algorithm's uniqueness guarantee becomes the limiting constraint rather than width alone.
