use super::model::WorkspaceMap;
use super::transitive_closure;

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

    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();

    // --- component checks ---
    // Build transitive closure: all components reachable from any base or project.
    // A component used only by another component (not directly by a base) is still "used".
    // Status uses raw dep-key identity (no interface alias resolution).
    let comp_deps: std::collections::HashMap<&str, &[String]> = map
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.deps.as_slice()))
        .collect();
    let seeds: Vec<String> = map
        .bases
        .iter()
        .flat_map(|b| b.deps.iter().cloned())
        .chain(map.projects.iter().flat_map(|p| p.deps.iter().cloned()))
        .collect();
    let depended_on = transitive_closure(
        seeds,
        |name| comp_deps.get(name).copied().unwrap_or(&[]).to_vec(),
        |dep_key| vec![dep_key.to_owned()],
    );

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
