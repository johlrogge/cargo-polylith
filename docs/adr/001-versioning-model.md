# ADR-001: Two-mode versioning: relaxed and strict

## Status
Accepted

## Context

Cargo requires every package to declare a `version` field. In a polylith workspace this creates friction: agents and humans struggle with what version to put where, leading to improvised bumps and inconsistency. Polylith's architecture already provides the dependency graph and interface/implementation distinction needed to reason about versioning, but there is no versioning policy or tooling today.

Key constraints:
- Cargo mandates a version field in every `Cargo.toml` -- we can't avoid it.
- Projects are "just bricks" -- they are not special-cased for versioning.
- The workspace version is a bundle/distro version (like a Linux distro release), not a package version.
- Polylith owns generated files (project `Cargo.toml`, workspace root `Cargo.toml`). Component `Cargo.toml` files are user-owned.
- Generated `Cargo.toml` files should carry a "GENERATED, DO NOT EDIT" comment.

## Decision

Support two versioning modes, configured in `Polylith.toml`:

### Relaxed mode
- All brick versions equal the workspace version.
- Bricks use `version.workspace = true` in their `Cargo.toml`.
- The workspace version is the single source of truth, declared in `Polylith.toml`.
- `cargo polylith bump` bumps the workspace version; all bricks follow automatically.

### Strict mode
- Every brick owns its version in its own `Cargo.toml`.
- Brick versions are change-tracking signals, bumped during development (enforced by push hooks calling `cargo polylith` CLI).
- At release time, `cargo polylith bump` walks the dependency graph depth-first per project, compares brick versions at HEAD vs the last release tag, and determines the severity of change:
  - Brick changed interface files: major signal
  - Brick changed internals (not interface): minor or patch signal (informed by conventional commits if available, or prompted via MCP)
  - Brick only bumped due to transitive dependency change: patch signal
- Each project gets an independent semver recommendation based on its subtree.
- The workspace version is then bumped to reflect the overall release.

### Ownership boundaries

| What | Owned by | Version source |
|---|---|---|
| Brick `Cargo.toml` version | User/agent | The file itself |
| Workspace dependency versions | Polylith (generated) | `[workspace.dependencies]` in generated root |
| Project version (in generated `Cargo.toml`) | Polylith (generated) | `Polylith.toml` |
| Workspace/distro version | Polylith | `Polylith.toml` |

### Tooling

- **`cargo polylith bump`** (release time): Reads current workspace version from `Polylith.toml`, analyzes brick version deltas (strict mode: compares brick versions at HEAD vs last tag as a git reference point), walks graph, computes new version, writes to `Polylith.toml`, regenerates project `Cargo.toml` files.
- **`cargo polylith check-version`** (hook time): Validates that changed bricks have appropriate version bumps. Used in push hooks. Branch-aware policy: relaxed on `feature/` branches, strict on `release/` and `develop`.
- **MCP `bump` tool**: Exposes the same analysis to agents, so release instructions become "call the bump tool" rather than "bump all versions."

### Git-flow integration

The source of truth for the current workspace version is `Polylith.toml`, not git tags. Tags are created *from* that version by `gitflow release finish` / `gitflow hotfix finish` — they follow `Polylith.toml`, not the other way around.

In strict mode, tags serve as git reference points for comparing brick versions between releases (e.g., `git show v1.2.3:components/foo/Cargo.toml`), but they are never the version source. The tag prefix (e.g., `v`) is whatever the user configured during `git flow init` — no custom convention needed.

Branch-aware enforcement policy:
- **feature/ branches**: No version enforcement on push. Still iterating.
- **develop**: Warn on push if bricks are changed without version bumps.
- **release/ branches**: Full enforcement. All brick bumps must be settled.
- **master/main**: Only receives merges from release/ and hotfix/.

## Why

- **Agents improvise badly** -- without a tool, agents interpret "bump all versions" creatively and inconsistently. A single `cargo polylith bump` command eliminates improvisation.
- **Cargo can't be ignored** -- the version field is mandatory. Rather than fighting it, we use it as a change-tracking mechanism (strict) or keep it trivially managed (relaxed).
- **Teams graduate naturally** -- start with relaxed, move to strict when granularity matters. Matches polylith's "use before reuse" philosophy.
- **No new state needed** -- `Polylith.toml` (version source of truth) + git-flow tags (reference points for change comparison) + Cargo.toml versions + the dependency graph (already built) are all the inputs required. No custom tag convention to maintain.

## Alternatives considered

- **Per-component versions in Polylith.toml** -- would mean polylith writes to component Cargo.toml, violating the ownership boundary where component files are user-owned.
- **Single workspace version only** -- too coarse for workspaces with multiple independently-deployed projects.
- **Version analysis without component version tracking** -- would require complex code analysis or heavy reliance on conventional commits. Using the version field as signal is simpler and already enforced by Cargo.
- **A middle-ground policy between relaxed and strict** -- adds complexity with unclear benefit. Two clean extremes are easier to understand and implement.

## Consequences

- `Polylith.toml` gains a `[versioning]` section with a `policy` field (`relaxed` or `strict`).
- Generated `Cargo.toml` files must include a "GENERATED, DO NOT EDIT" header comment.
- The `bump` command becomes the canonical way to manage versions at release time.
- Push hooks become the enforcement mechanism during development (strict mode).
- Agents interacting via MCP use the `bump` tool instead of manually editing version fields.
