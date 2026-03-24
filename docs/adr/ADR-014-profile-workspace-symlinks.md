# ADR-014: Profile Workspaces Use Symlinks to Satisfy Cargo Hierarchy Requirement

## Status
Accepted

## Context
Cargo 1.94+ requires that all workspace members reside hierarchically below the workspace root directory. Profile workspaces live at `profiles/<name>/`, while bricks live at `components/`, `bases/`, and `projects/` at the repository root — peers, not descendants. Using direct relative paths such as `../../components/foo` was tried and rejected by Cargo's path validation. Writing a separate copy of each brick per profile was ruled out as prohibitively complex. An alternative approach (Option C) of rewriting the root `Cargo.toml` on `change-profile` was considered but judged more complex than the chosen solution.

## Decision
Each profile directory contains three symlinks created at `profile migrate` time: `components → ../../components`, `bases → ../../bases`, and `projects → ../../projects`. The profile workspace's `Cargo.toml` references members as `components/foo`, `bases/bar`, etc., which are paths that sit hierarchically below `profiles/<name>/` from Cargo's perspective. Cargo follows the symlinks to reach the real source, and the walk-up from the real path finds no `[workspace]` in the root `Cargo.toml` (which becomes a placeholder `[package]` after migration), allowing the profile workspace to claim the bricks as its own members.

## Consequences
Profile workspaces are valid under Cargo 1.94+ without duplicating source files. Switching profiles is a matter of selecting a different `profiles/<name>/Cargo.toml`. The symlinks must be present for the workspace to build; `cargo polylith profile migrate` creates them, and the `.gitignore` treatment of symlinks must be considered in version-controlled repositories. The approach depends on the root `Cargo.toml` not containing a `[workspace]` section after migration — this coupling is documented in ADR-015.
