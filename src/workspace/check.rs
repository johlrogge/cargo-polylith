use serde::Serialize;

use super::model::WorkspaceMap;

/// A single violation found during `check`.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub kind: ViolationKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// A component is missing its expected lib.rs re-export file.
    MissingLibRs,
    /// A component is missing its implementation file (`src/<name>.rs`).
    MissingImplFile,
    /// A base is missing its `src/lib.rs` — bases must expose a runtime API as a library.
    BaseMissingLibRs,
    /// A base has a `src/main.rs` — executable entry points belong in projects, not bases.
    BaseHasMainRs,
    /// A base depends on another base (only components are allowed as deps).
    BaseDepOnBase,
    /// A component is not depended on by any base or project (potential dead code).
    OrphanComponent,
    /// A component's lib.rs uses a wildcard re-export (`pub use <name>::*`).
    WildcardReExport,
    /// A project has no dependency on any base.
    ProjectMissingBase,
    /// A component or base exists in its polylith directory but is not listed in the root
    /// workspace members, so `cargo build --workspace` will silently ignore it.
    NotInRootWorkspace,
    /// Two or more components declare the same interface name but none has a package name
    /// matching the interface — every consumer must `[patch]` explicitly (no default impl).
    AmbiguousInterface,
    /// Two or more components share the same package name — likely a stub that was named
    /// identically to the real component instead of getting a distinct name.
    DuplicateName,
    /// A component has no `interface` declared in `[package.metadata.polylith]`.
    MissingInterface,
}

/// Run all structural checks against `map` and return any violations found.
pub fn run_checks(map: &WorkspaceMap) -> Vec<Violation> {
    let mut violations = vec![];

    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();

    // Transitive closure: all components reachable from any base or project.
    let comp_deps: std::collections::HashMap<&str, &[String]> = map
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.deps.as_slice()))
        .collect();
    let mut depended_on: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<&str> = map
        .bases
        .iter()
        .flat_map(|b| b.deps.iter().map(|d| d.as_str()))
        .chain(map.projects.iter().flat_map(|p| {
            p.deps.iter().map(|dep_name| {
                // If this dep is replaced by a patch, seed the BFS with the patched
                // component's name so the stub is counted as reachable (not orphaned).
                p.patches
                    .iter()
                    .find(|(patched_dep, _)| patched_dep == dep_name)
                    .and_then(|(_, patch_path)| {
                        map.components
                            .iter()
                            .chain(map.bases.iter())
                            .find(|brick| brick.path == *patch_path)
                            .map(|brick| brick.name.as_str())
                    })
                    .unwrap_or(dep_name.as_str())
            })
        }))
        .collect();
    while let Some(name) = queue.pop_front() {
        if depended_on.insert(name) {
            if let Some(deps) = comp_deps.get(name) {
                for d in *deps {
                    queue.push_back(d.as_str());
                }
            }
        }
    }

    // --- component checks ---
    for comp in &map.components {
        let lib_rs = comp.path.join("src/lib.rs");
        if !lib_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::MissingLibRs,
                message: format!("component '{}': src/lib.rs is missing", comp.name),
            });

            // No lib.rs and no impl file → also flag MissingImplFile
            let impl_file = comp.path.join("src").join(format!("{}.rs", comp.name));
            if !impl_file.exists() {
                violations.push(Violation {
                    kind: ViolationKind::MissingImplFile,
                    message: format!(
                        "component '{}': src/{}.rs is missing",
                        comp.name, comp.name
                    ),
                });
            }
        } else {
            let content = std::fs::read_to_string(&lib_rs).unwrap_or_default();
            // Rust normalises hyphens to underscores in module/crate names.
            let rust_name = comp.name.replace('-', "_");
            let wildcard = format!("pub use {}::*", rust_name);

            if content.contains(&wildcard) {
                violations.push(Violation {
                    kind: ViolationKind::WildcardReExport,
                    message: format!(
                        "component '{}': lib.rs uses wildcard re-export — consider explicit `pub use {}::{{Type, fn}};`",
                        comp.name, rust_name
                    ),
                });
            }
            // If lib.rs exists, any layout (flat, submodule, re-export from deps) is valid.
        }

        if !depended_on.contains(comp.name.as_str()) {
            violations.push(Violation {
                kind: ViolationKind::OrphanComponent,
                message: format!("component '{}' is not used by any base or project", comp.name),
            });
        }
    }

    // --- project checks ---
    for project in &map.projects {
        let has_base_dep = project.deps.iter().any(|d| base_names.contains(d.as_str()));
        if !has_base_dep && !project.test_project {
            violations.push(Violation {
                kind: ViolationKind::ProjectMissingBase,
                message: format!(
                    "project '{}' has no base dependency — deliverable projects must include at least one base; set `[package.metadata.polylith] test-project = true` to suppress for test/dev projects",
                    project.name
                ),
            });
        }
    }

    // --- duplicate name checks ---
    // Two bricks with the same package name means a stub was mis-named. Cargo would
    // reject both in the same workspace; even if only one is currently a member, the
    // duplication signals a configuration error.
    let mut by_name: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for brick in map.components.iter().chain(map.bases.iter()) {
        by_name.entry(brick.name.as_str()).or_default().push(
            brick.path.strip_prefix(&map.root)
                .map(|p| p.to_str().unwrap_or("?"))
                .unwrap_or("?"),
        );
    }
    for (name, paths) in &by_name {
        if paths.len() > 1 {
            violations.push(Violation {
                kind: ViolationKind::DuplicateName,
                message: format!(
                    "package name '{}' is used by {} bricks ({}) — give each a distinct name and declare `[package.metadata.polylith] interface = \"{}\"` on both",
                    name, paths.len(), paths.join(", "), name
                ),
            });
        }
    }

    // --- missing interface annotation ---
    for comp in &map.components {
        if comp.interface.is_none() {
            violations.push(Violation {
                kind: ViolationKind::MissingInterface,
                message: format!(
                    "component '{}' has no `[package.metadata.polylith] interface = \"...\"` \
                     declaration — add interface metadata or run `cargo polylith edit` to set it",
                    comp.name
                ),
            });
        }
    }

    // --- interface checks ---
    // Group components by declared interface name. Warn when multiple components share
    // an interface but none has a package name matching the interface (no default impl).
    let mut by_interface: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for comp in &map.components {
        if let Some(iface) = comp.interface.as_deref() {
            by_interface.entry(iface).or_default().push(comp.name.as_str());
        }
    }
    for (iface, impls) in &by_interface {
        if impls.len() > 1 && !impls.iter().any(|n| *n == *iface) {
            violations.push(Violation {
                kind: ViolationKind::AmbiguousInterface,
                message: format!(
                    "interface '{}' has {} implementations ({}) but none has the default package name — every consumer must [patch] explicitly",
                    iface,
                    impls.len(),
                    impls.join(", ")
                ),
            });
        }
    }

    // --- workspace membership checks ---
    if !map.root_members.is_empty() {
        for brick in map.components.iter().chain(map.bases.iter()) {
            let rel = brick
                .path
                .strip_prefix(&map.root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if !map.root_members.iter().any(|m| member_covers(m, &rel)) {
                let kind_label = match brick.kind {
                    super::model::BrickKind::Component => "component",
                    super::model::BrickKind::Base => "base",
                };
                violations.push(Violation {
                    kind: ViolationKind::NotInRootWorkspace,
                    message: format!(
                        "{kind_label} '{}' is not listed in root workspace members \
                         — add '{rel}' to [workspace] members in Cargo.toml",
                        brick.name
                    ),
                });
            }
        }
    }

    // --- base checks ---
    for base in &map.bases {
        let lib_rs  = base.path.join("src/lib.rs");
        let main_rs = base.path.join("src/main.rs");

        if !lib_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::BaseMissingLibRs,
                message: format!(
                    "base '{}': src/lib.rs is missing — bases must expose a runtime API as a library function",
                    base.name
                ),
            });
        }

        if main_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::BaseHasMainRs,
                message: format!(
                    "base '{}': src/main.rs should be in a project, not a base — bases expose library functions like `run()` that projects call",
                    base.name
                ),
            });
        }

        for dep in &base.deps {
            if base_names.contains(dep.as_str()) {
                violations.push(Violation {
                    kind: ViolationKind::BaseDepOnBase,
                    message: format!(
                        "base '{}' depends on base '{}' — bases may only depend on components",
                        base.name, dep
                    ),
                });
            }
        }
    }

    violations
}

/// Returns true if `pattern` (a root workspace members entry) covers `rel_path`
/// (a path relative to the workspace root, using `/` separators).
///
/// Handles the two common forms:
/// - `"components/*"` — matches any direct child of `components/`
/// - `"components/foo"` — exact match
fn member_covers(pattern: &str, rel_path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        rel_path
            .strip_prefix(&format!("{prefix}/"))
            .map(|rest| !rest.contains('/'))
            .unwrap_or(false)
    } else {
        rel_path == pattern
    }
}
