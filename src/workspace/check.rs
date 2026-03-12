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
    /// A component's lib.rs doesn't contain the expected `pub use <name>::*` re-export.
    MissingReExport,
    /// A component is missing its implementation file (`src/<name>.rs`).
    MissingImplFile,
    /// A base is missing its `src/main.rs`.
    MissingMainRs,
    /// A base depends on another base (only components are allowed as deps).
    BaseDepOnBase,
    /// A component is not depended on by any base (potential dead code).
    OrphanComponent,
}

/// Run all structural checks against `map` and return any violations found.
pub fn run_checks(map: &WorkspaceMap) -> Vec<Violation> {
    let mut violations = vec![];

    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();
    let depended_on: std::collections::HashSet<&str> = map
        .bases
        .iter()
        .flat_map(|b| b.deps.iter().map(|d| d.as_str()))
        .collect();

    // --- component checks ---
    for comp in &map.components {
        let lib_rs = comp.path.join("src/lib.rs");
        if !lib_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::MissingLibRs,
                message: format!(
                    "component '{}': src/lib.rs is missing",
                    comp.name
                ),
            });
        } else {
            let content = std::fs::read_to_string(&lib_rs).unwrap_or_default();
            let expected = format!("pub use {}::*", comp.name);
            if !content.contains(&expected) {
                violations.push(Violation {
                    kind: ViolationKind::MissingReExport,
                    message: format!(
                        "component '{}': src/lib.rs missing `{};`",
                        comp.name, expected
                    ),
                });
            }
        }

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

        if !depended_on.contains(comp.name.as_str()) {
            violations.push(Violation {
                kind: ViolationKind::OrphanComponent,
                message: format!(
                    "component '{}' is not used by any base",
                    comp.name
                ),
            });
        }
    }

    // --- base checks ---
    for base in &map.bases {
        let main_rs = base.path.join("src/main.rs");
        if !main_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::MissingMainRs,
                message: format!("base '{}': src/main.rs is missing", base.name),
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
