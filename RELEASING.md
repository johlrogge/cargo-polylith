# Release Workflow — cargo-polylith

cargo-polylith uses git flow with a multi-agent pre-release checklist.

## Branch Model

- `master` — released code only, always tagged
- `develop` — integration branch, features merge here
- `feature/*` — individual features, branch from develop
- `release/*` — release prep, branch from develop
- `hotfix/*` — urgent fixes, branch from master

## Versioning (semver)

- `feat` commits → minor bump (0.4.0 → 0.5.0)
- `fix` / `chore` commits → patch bump (0.5.0 → 0.5.1)
- Breaking change (`!`) → major bump (0.5.0 → 1.0.0)

Version lives in `Cargo.toml` at the workspace root.

## Release Checklist

Run these steps in order before cutting a release:

1. **cargo test** — gate; must pass before anything else proceeds
   ```
   cargo test
   ```
   Do not continue if any test fails.

2. **architect** — review the diff since last release for correctness and quality
   > "Review changes since last release"
   Reads `.claude/skills/architect/SKILL.md`.

3. **documenter** — update README.md and ROADMAP.md to reflect the release
   > "Update docs for release 0.x.x"

3.5. **Review skill template** — check whether any new violation kinds or MCP tools were
   added since the last release. If so, update `src/scaffold/claude_skill.md` accordingly
   (violation model section, scaffolding table, rules). This keeps the generated skill
   accurate for users who regenerate after upgrading.

4. **commit** — commit any documentation changes
   > "Commit doc updates for release 0.x.x"
   The commit agent reads `.claude/skills/conventional-commits/SKILL.md` for format.

5. **devops** — start the release branch
   > "Start release 0.x.x"
   Uses `gitflow_release_start`.

6. **Version bump** — update the version field in `Cargo.toml`, then run `cargo check` to
   regenerate `Cargo.lock`.
   ```
   # edit Cargo.toml: version = "0.x.x"
   cargo check
   ```

7. **commit** — commit the version bump
   > "Commit version bump to 0.x.x"
   Expected message: `chore(release): bump version to 0.x.x`

8. **devops** — finish the release (merge to master, tag, merge back to develop)
   > "Finish release 0.x.x"
   Always confirm with devops before finishing.

9. **Human** — push to remote
   ```
   git push origin master develop --tags
   ```

## Feature Checklist

1. **devops** — start feature branch
   > "Start feature <feature-name>"
   Uses `gitflow_feature_start`. Branches from `develop`.

2. **Implement** — write code on `feature/<feature-name>`
   Follow conventions in `CLAUDE.md`.

3. **cargo test** — must pass before finishing
   ```
   cargo test
   ```

4. **commit** — commit all changes
   > "Commit feature <feature-name>"
   The commit agent reads `.claude/skills/conventional-commits/SKILL.md` for format.

5. **devops** — finish feature (merges into develop)
   > "Finish feature <feature-name>"
   Always confirm with devops before finishing.

6. **Human** — push develop to remote (optional, at your discretion)
   ```
   git push origin develop
   ```

## Hotfix Checklist

1. **devops** — start hotfix
2. **cargo test** — confirm tests still pass
3. **commit** — commit the fix
4. **devops** — finish hotfix (confirm before calling)
5. **Human** — push

## Notes

- Agents never push — that always stays with the human
- Always confirm with devops before finishing a release or hotfix
- The commit agent reads `.claude/skills/conventional-commits/SKILL.md` for format
- All analysis is pure TOML + filesystem; no `cargo metadata` invocation needed during release
