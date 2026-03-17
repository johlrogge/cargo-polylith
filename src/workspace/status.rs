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
    let depended_on: std::collections::HashSet<&str> = map
        .bases
        .iter()
        .flat_map(|b| b.deps.iter().map(|d| d.as_str()))
        .collect();

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
                "component '{}' is not used by any base",
                comp.name
            ));
        }
    }

    if explicit_count > 0 {
        confirmed.push(format!("{} component(s) use explicit re-exports", explicit_count));
    }

    StatusReport { confirmed, divergences, suggestions }
}
