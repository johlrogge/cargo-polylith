use super::model::WorkspaceMap;

#[derive(Debug)]
pub struct StatusReport {
    pub confirmed: Vec<String>,
    pub divergences: Vec<Divergence>,
    pub suggestions: Vec<String>,
}

#[derive(Debug)]
pub struct Divergence {
    pub observation: String,
    pub suggestion: String,
}

pub fn run_status(map: &WorkspaceMap) -> StatusReport {
    let mut confirmed = vec![];
    let mut divergences = vec![];
    let mut suggestions = vec![];

    // --- workspace-level ---
    if !map.components.is_empty() {
        confirmed.push(format!("components/ ({} crates)", map.components.len()));
    } else {
        suggestions.push("No components/ directory or no components found".to_string());
    }

    if !map.bases.is_empty() {
        confirmed.push(format!("bases/ ({} crates)", map.bases.len()));
    } else {
        suggestions.push("No bases/ directory or no bases found".to_string());
    }

    if map.projects.is_empty() {
        suggestions.push(
            "No projects/ directory found\n    run: cargo polylith project new <name>".to_string(),
        );
    }

    // base-depends-on-base
    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();
    let has_base_dep_base = map.bases.iter().any(|b| b.deps.iter().any(|d| base_names.contains(d.as_str())));
    if has_base_dep_base {
        suggestions.push("Some base(s) depend on other bases — bases may only depend on components".to_string());
    } else {
        confirmed.push("No base-depends-on-base".to_string());
    }

    // --- component checks ---
    // Build transitive closure: all components reachable from any base.
    // A component used only by another component (not directly by a base) is still "used".
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

    let mut explicit_count = 0usize;

    for comp in &map.components {
        let lib_rs = comp.path.join("src/lib.rs");

        if !lib_rs.exists() {
            suggestions.push(format!(
                "component '{}': src/lib.rs is missing — add a lib.rs with re-exports",
                comp.name
            ));
        } else {
            let content = std::fs::read_to_string(&lib_rs).unwrap_or_default();
            // Rust normalises hyphens to underscores in module/crate names.
            let rust_name = comp.name.replace('-', "_");
            let wildcard = format!("pub use {}::*", rust_name);
            let any_reexport = format!("pub use {}::", rust_name);

            if content.contains(&wildcard) {
                divergences.push(Divergence {
                    observation: format!(
                        "component '{}': lib.rs uses wildcard re-export",
                        comp.name
                    ),
                    suggestion: format!(
                        "consider explicit `pub use {}::{{Type, fn}};` instead of `pub use {}::*;`",
                        rust_name, rust_name
                    ),
                });
            } else if content.contains(&any_reexport) {
                explicit_count += 1;
            }
            // else: no re-export at all; status is lenient, skip


        }

        if !depended_on.contains(comp.name.as_str()) {
            suggestions.push(format!(
                "component '{}' is not used by any base or project",
                comp.name
            ));
        }
    }

    if explicit_count > 0 {
        confirmed.push(format!("{} component(s) use explicit re-exports", explicit_count));
    }

    // --- base structural checks ---
    for base in &map.bases {
        let lib_rs  = base.path.join("src/lib.rs");
        let main_rs = base.path.join("src/main.rs");
        if !lib_rs.exists() {
            suggestions.push(format!(
                "base '{}': src/lib.rs is missing — add a lib.rs exposing a runtime API function",
                base.name
            ));
        }
        if main_rs.exists() {
            divergences.push(Divergence {
                observation: format!("base '{}': has src/main.rs", base.name),
                suggestion: "move the executable entry point to a project; bases should only expose library functions like `run()`".to_string(),
            });
        }
    }

    // --- project checks ---
    for project in &map.projects {
        let has_base_dep = project.deps.iter().any(|d| base_names.contains(d.as_str()));
        if !has_base_dep {
            suggestions.push(format!(
                "project '{}' has no base dependency — polylith projects must include at least one base",
                project.name
            ));
        }
    }

    StatusReport { confirmed, divergences, suggestions }
}
