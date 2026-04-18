use std::path::Path;

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

fn detect_old_model_profile_dirs(root: &Path) -> Vec<String> {
    let profiles_dir = root.join("profiles");
    if !profiles_dir.exists() {
        return vec![];
    }
    // Best-effort: silently skip unreadable entries — this is a hint, not a check.
    let mut names: Vec<String> = std::fs::read_dir(&profiles_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub fn run_status(map: &WorkspaceMap) -> StatusReport {
    let mut confirmed = vec![];
    let mut divergences = vec![];
    let mut suggestions = vec![];

    // --- old-model profile directory detection ---
    for name in detect_old_model_profile_dirs(&map.root) {
        suggestions.push(format!(
            "old-model profile directory detected at profiles/{name}/\n    run: cargo polylith change-profile {name}"
        ));
    }

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use tempfile::TempDir;

    use super::super::model::WorkspaceMap;
    use super::{detect_old_model_profile_dirs, run_status};

    fn empty_map(root: &std::path::Path) -> WorkspaceMap {
        WorkspaceMap {
            root: root.to_path_buf(),
            components: vec![],
            bases: vec![],
            projects: vec![],
            root_members: vec![],
            is_workspace: true,
            root_workspace_deps: HashMap::new(),
            root_workspace_interface_deps: HashMap::new(),
            polylith_toml: None,
            root_package_meta: None,
            component_by_name: HashMap::new(),
            component_by_interface: HashMap::new(),
            base_by_name: HashMap::new(),
        }
    }

    // --- detect_old_model_profile_dirs ---

    #[test]
    fn detect_returns_empty_when_profiles_dir_missing() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(detect_old_model_profile_dirs(tmp.path()), Vec::<String>::new());
    }

    #[test]
    fn detect_returns_empty_for_pure_new_model() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("profiles")).unwrap();
        fs::write(tmp.path().join("profiles/dev.profile"), "").unwrap();
        assert_eq!(detect_old_model_profile_dirs(tmp.path()), Vec::<String>::new());
    }

    #[test]
    fn detect_reports_old_model_dir() {
        let tmp = TempDir::new().unwrap();
        let dev_dir = tmp.path().join("profiles/dev");
        fs::create_dir_all(&dev_dir).unwrap();
        fs::write(dev_dir.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        assert_eq!(detect_old_model_profile_dirs(tmp.path()), vec!["dev".to_string()]);
    }

    #[test]
    fn detect_reports_multiple_dirs_sorted() {
        let tmp = TempDir::new().unwrap();
        for name in &["prod", "dev"] {
            let dir = tmp.path().join("profiles").join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        }
        assert_eq!(
            detect_old_model_profile_dirs(tmp.path()),
            vec!["dev".to_string(), "prod".to_string()]
        );
    }

    #[test]
    fn detect_ignores_dir_without_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("profiles/foo")).unwrap();
        assert_eq!(detect_old_model_profile_dirs(tmp.path()), Vec::<String>::new());
    }

    #[test]
    fn detect_reports_only_old_model_in_mixed_workspace() {
        let tmp = TempDir::new().unwrap();
        // Old-model: profiles/dev/ with a Cargo.toml
        let dev_dir = tmp.path().join("profiles/dev");
        fs::create_dir_all(&dev_dir).unwrap();
        fs::write(dev_dir.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        // New-model: profiles/prod.profile (a plain file, not a dir with Cargo.toml)
        fs::write(tmp.path().join("profiles/prod.profile"), "").unwrap();
        assert_eq!(
            detect_old_model_profile_dirs(tmp.path()),
            vec!["dev".to_string()]
        );
    }

    // --- run_status integration ---

    #[test]
    fn run_status_emits_hint_for_old_model() {
        let tmp = TempDir::new().unwrap();
        let dev_dir = tmp.path().join("profiles/dev");
        fs::create_dir_all(&dev_dir).unwrap();
        fs::write(dev_dir.join("Cargo.toml"), "[workspace]\nmembers = []\nresolver = \"2\"\n").unwrap();

        let map = empty_map(tmp.path());
        let report = run_status(&map);

        assert!(
            report.suggestions.iter().any(|s| s.contains("profiles/dev") && s.contains("change-profile dev")),
            "expected old-model hint in suggestions, got: {:?}",
            report.suggestions
        );
    }
}
