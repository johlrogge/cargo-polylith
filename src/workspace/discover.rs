use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_toml::Manifest;

use super::model::{Brick, BrickKind, ExternalDepInfo, Project, WorkspaceMap};

/// Resolve the workspace root: use `override_root` if given, otherwise walk
/// up from `start` searching for a `Cargo.toml` with `[workspace]`.
pub fn resolve_root(start: &Path, override_root: Option<&Path>) -> Result<PathBuf> {
    match override_root {
        Some(p) => {
            let abs = if p.is_absolute() {
                p.to_path_buf()
            } else {
                start.join(p)
            };
            anyhow::ensure!(
                abs.join("Cargo.toml").exists(),
                "no Cargo.toml found at workspace root '{}'",
                abs.display()
            );
            Ok(abs)
        }
        None => find_workspace_root(start),
    }
}

/// Walk up from `start` to find the workspace root Cargo.toml (the one with `[workspace]`).
/// If no workspace Cargo.toml is found, returns the nearest directory containing any
/// Cargo.toml, or `start` itself — callers should check `WorkspaceMap::is_workspace`.
pub fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut current = start.to_path_buf();
    let mut fallback: Option<PathBuf> = None;
    loop {
        let candidate = current.join("Cargo.toml");
        if candidate.exists() {
            let manifest = Manifest::from_path(&candidate)
                .with_context(|| format!("failed to parse {}", candidate.display()))?;
            if manifest.workspace.is_some() {
                return Ok(current);
            }
            if fallback.is_none() {
                fallback = Some(current.clone());
            }
        }
        if !current.pop() {
            return Ok(fallback.unwrap_or_else(|| start.to_path_buf()));
        }
    }
}

/// Build a `WorkspaceMap` from the given workspace root.
pub fn build_workspace_map(root: &Path) -> Result<WorkspaceMap> {
    let components = scan_bricks(root, BrickKind::Component)?;
    let bases = scan_bricks(root, BrickKind::Base)?;
    let projects = scan_projects(root)?;
    let (root_members, is_workspace, root_workspace_deps) = {
        let toml_path = root.join("Cargo.toml");
        if toml_path.exists() {
            let manifest = Manifest::from_path(&toml_path)
                .with_context(|| format!("failed to parse {}", toml_path.display()))?;
            let is_ws = manifest.workspace.is_some();
            let members = manifest
                .workspace
                .as_ref()
                .map(|ws| ws.members.clone())
                .unwrap_or_default();
            let ws_deps = manifest
                .workspace
                .map(|ws| {
                    ws.dependencies
                        .into_iter()
                        .filter_map(|(key, dep)| {
                            // Skip path deps — they're internal, no drift possible
                            if dep.detail().and_then(|d| d.path.as_deref()).is_some() {
                                return None;
                            }
                            let version = dep
                                .detail()
                                .and_then(|d| d.version.as_deref())
                                .or_else(|| {
                                    let r = dep.req();
                                    if r != "*" { Some(r) } else { None }
                                })
                                .map(|v| v.to_string());
                            let mut features: Vec<String> = dep
                                .detail()
                                .map(|d| d.features.clone())
                                .unwrap_or_default();
                            features.sort();
                            Some((key, ExternalDepInfo { features, version }))
                        })
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .unwrap_or_default();
            (members, is_ws, ws_deps)
        } else {
            (vec![], false, std::collections::HashMap::new())
        }
    };
    Ok(WorkspaceMap {
        root: root.to_path_buf(),
        components,
        bases,
        projects,
        root_members,
        is_workspace,
        root_workspace_deps,
    })
}

fn brick_dir(root: &Path, kind: &BrickKind) -> PathBuf {
    match kind {
        BrickKind::Component => root.join("components"),
        BrickKind::Base => root.join("bases"),
    }
}

fn scan_bricks(root: &Path, kind: BrickKind) -> Result<Vec<Brick>> {
    let dir = brick_dir(root, &kind);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut bricks = vec![];
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = Manifest::from_path(&manifest_path)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let name = manifest
            .package
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let deps = manifest
            .dependencies
            .keys()
            .cloned()
            .collect();
        let interface = fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
            .and_then(|doc| {
                doc.get("package")
                    .and_then(|p| p.get("metadata"))
                    .and_then(|m| m.get("polylith"))
                    .and_then(|p| p.get("interface"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
        bricks.push(Brick {
            name,
            kind: kind.clone(),
            path: path.clone(),
            deps,
            manifest_path,
            interface,
        });
    }
    bricks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(bricks)
}

fn scan_projects(root: &Path) -> Result<Vec<Project>> {
    let dir = root.join("projects");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut projects = vec![];
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }
        let dir_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let content = match fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: skipping {}: {e}", manifest_path.display());
                continue;
            }
        };
        let doc: toml_edit::DocumentMut = match content.parse() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("warning: skipping {}: {e}", manifest_path.display());
                continue;
            }
        };
        let name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
            .unwrap_or(dir_name);
        // Resolve a dep entry (key, value) to the actual package name.
        // If `package = "..."` is set, that is the real crate name being pulled in;
        // the key is just a local alias. This applies to both [dependencies] and
        // [workspace.dependencies].
        let resolve_pkg_name = |k: &str, v: &toml_edit::Item| -> String {
            let pkg = v
                .as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("package"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    v.as_table()
                        .and_then(|t| t.get("package"))
                        .and_then(|v| v.as_value())
                        .and_then(|v| v.as_str())
                });
            pkg.unwrap_or(k).to_string()
        };
        // Helper: extract the `path = "..."` value from a dep item (inline table or regular table).
        let extract_path = |v: &toml_edit::Item| -> Option<String> {
            v.as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    v.as_table()
                        .and_then(|t| t.get("path"))
                        .and_then(|v| v.as_value())
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
        };
        // Helper: check whether a dep item has an explicit `package = "..."` alias.
        let has_package_alias = |v: &toml_edit::Item| -> bool {
            v.as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("package"))
                .is_some()
                || v.as_table()
                    .and_then(|t| t.get("package"))
                    .is_some()
        };
        let mut dep_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut dep_paths: Vec<(String, PathBuf)> = vec![];
        let mut external_deps: std::collections::HashMap<String, ExternalDepInfo> =
            std::collections::HashMap::new();

        // Helper: check whether a dep item has `workspace = true`.
        let is_workspace_dep = |v: &toml_edit::Item| -> bool {
            v.as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("workspace"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || v.as_table()
                    .and_then(|t| t.get("workspace"))
                    .and_then(|v| v.as_value())
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
        };

        // Helper: extract version string from a dep item (bare string or `version = "..."` key).
        let extract_version = |v: &toml_edit::Item| -> Option<String> {
            // bare string: `serde = "1.0"`
            if let Some(s) = v.as_value().and_then(|v| v.as_str()) {
                if s != "*" {
                    return Some(s.to_string());
                }
                return None;
            }
            // inline table: `serde = { version = "1.0", ... }`
            let from_inline = v
                .as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("version"))
                .and_then(|v| v.as_str())
                .filter(|s| *s != "*")
                .map(|s| s.to_string());
            if from_inline.is_some() {
                return from_inline;
            }
            // regular table
            v.as_table()
                .and_then(|t| t.get("version"))
                .and_then(|v| v.as_value())
                .and_then(|v| v.as_str())
                .filter(|s| *s != "*")
                .map(|s| s.to_string())
        };

        // Helper: extract features list from a dep item.
        let extract_features = |v: &toml_edit::Item| -> Vec<String> {
            let arr = v
                .as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("features"))
                // InlineTable::get returns &toml_edit::Value, so call as_array() directly
                .and_then(|v| v.as_array())
                .or_else(|| {
                    v.as_table()
                        .and_then(|t| t.get("features"))
                        .and_then(|v| v.as_value())
                        .and_then(|v| v.as_array())
                });
            let mut feats: Vec<String> = arr
                .map(|a| {
                    a.iter()
                        .filter_map(|f| f.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            feats.sort();
            feats
        };

        // [dependencies] — direct deps of the project binary
        if let Some(t) = doc.get("dependencies").and_then(|t| t.as_table()) {
            for (k, v) in t.iter() {
                dep_set.insert(resolve_pkg_name(k, v));
                if !has_package_alias(v) {
                    if let Some(rel) = extract_path(v) {
                        dep_paths.push((k.to_string(), path.join(&rel)));
                    } else if !is_workspace_dep(v) {
                        // External dep — capture features and version for drift checks.
                        let features = extract_features(v);
                        let version = extract_version(v);
                        external_deps.insert(k.to_string(), ExternalDepInfo { features, version });
                    }
                }
            }
        }
        // [workspace.dependencies] — inherited by bases listed as workspace members;
        // components resolved here (especially renamed ones) are active in this project.
        if let Some(t) = doc
            .get("workspace")
            .and_then(|ws| ws.get("dependencies"))
            .and_then(|t| t.as_table())
        {
            for (k, v) in t.iter() {
                dep_set.insert(resolve_pkg_name(k, v));
                if !has_package_alias(v) {
                    if let Some(rel) = extract_path(v) {
                        dep_paths.push((k.to_string(), path.join(&rel)));
                    }
                }
            }
        }
        let deps: Vec<String> = dep_set.into_iter().collect();
        let members = doc
            .get("workspace")
            .and_then(|ws| ws.get("members"))
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| path.join(s))
                    .collect()
            })
            .unwrap_or_default();
        let patches: Vec<(String, PathBuf)> = doc
            .get("patch")
            .and_then(|p| p.get("crates-io"))
            .and_then(|ci| ci.as_table())
            .map(|t| {
                t.iter()
                    .filter_map(|(dep_name, item)| {
                        // path = "..." may be in an inline table or a regular table
                        let rel = item
                            .as_value()
                            .and_then(|v| v.as_inline_table())
                            .and_then(|t| t.get("path"))
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                item.as_table()
                                    .and_then(|t| t.get("path"))
                                    .and_then(|v| v.as_value())
                                    .and_then(|v| v.as_str())
                            })?;
                        let abs = path.join(rel);
                        let canonical = std::fs::canonicalize(&abs).ok()?;
                        Some((dep_name.to_string(), canonical))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let test_project = doc
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("polylith"))
            .and_then(|p| p.get("test-project"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        projects.push(Project {
            name,
            path: path.clone(),
            deps,
            members,
            patches,
            test_project,
            dep_paths,
            external_deps,
        });
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/poly-ws")
    }

    #[test]
    fn finds_workspace_root_from_subdir() {
        // Start inside a component subdir — should still find the root.
        let start = fixture().join("components/logger");
        let root = find_workspace_root(&start).unwrap();
        assert_eq!(root, fixture());
    }

    #[test]
    fn finds_workspace_root_from_root() {
        let root = find_workspace_root(&fixture()).unwrap();
        assert_eq!(root, fixture());
    }

    #[test]
    fn builds_workspace_map_components() {
        let map = build_workspace_map(&fixture()).unwrap();
        let names: Vec<_> = map.components.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"logger"), "logger missing: {names:?}");
        assert!(names.contains(&"parser"), "parser missing: {names:?}");
    }

    #[test]
    fn builds_workspace_map_bases() {
        let map = build_workspace_map(&fixture()).unwrap();
        assert_eq!(map.bases.len(), 1);
        assert_eq!(map.bases[0].name, "cli");
    }

    #[test]
    fn builds_workspace_map_projects() {
        let map = build_workspace_map(&fixture()).unwrap();
        assert_eq!(map.projects.len(), 1);
        assert_eq!(map.projects[0].name, "main-project");
    }

    #[test]
    fn base_deps_include_components() {
        let map = build_workspace_map(&fixture()).unwrap();
        let cli = map.bases.iter().find(|b| b.name == "cli").unwrap();
        assert!(cli.deps.contains(&"parser".to_string()));
        assert!(cli.deps.contains(&"logger".to_string()));
    }

    #[test]
    fn components_sorted_alphabetically() {
        let map = build_workspace_map(&fixture()).unwrap();
        let names: Vec<_> = map.components.iter().map(|b| b.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn missing_dirs_return_empty() {
        use tempfile::TempDir;
        use std::fs;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers=[]\nresolver=\"2\"\n",
        )
        .unwrap();
        let map = build_workspace_map(dir.path()).unwrap();
        assert!(map.components.is_empty());
        assert!(map.bases.is_empty());
        assert!(map.projects.is_empty());
    }
}
