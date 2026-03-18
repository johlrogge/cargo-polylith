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
        .chain(map.projects.iter().flat_map(|p| p.deps.iter().map(|d| d.as_str())))
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
        if !has_base_dep {
            violations.push(Violation {
                kind: ViolationKind::ProjectMissingBase,
                message: format!(
                    "project '{}' has no base dependency — polylith projects must include at least one base",
                    project.name
                ),
            });
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
