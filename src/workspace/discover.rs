use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_toml::Manifest;

use super::model::{Brick, BrickKind, ExternalDepInfo, PolylithToml, Profile, Project, WorkspaceMap, WorkspacePackageMeta, WorkspacePathDep};

/// Resolve the workspace root: use `override_root` if given, otherwise walk
/// up from `start` searching for a `Polylith.toml` or `Cargo.toml` with `[workspace]`.
pub fn resolve_root(start: &Path, override_root: Option<&Path>) -> Result<PathBuf> {
    match override_root {
        Some(p) => {
            let abs = if p.is_absolute() {
                p.to_path_buf()
            } else {
                start.join(p)
            };
            anyhow::ensure!(
                abs.join("Polylith.toml").exists() || abs.join("Cargo.toml").exists(),
                "no Polylith.toml or Cargo.toml found at workspace root '{}'",
                abs.display()
            );
            Ok(abs)
        }
        None => find_workspace_root(start),
    }
}

/// Walk up from `start` to find the workspace root.
///
/// Preferred marker: `Polylith.toml` — return that directory immediately if found.
/// Fallback: `Cargo.toml` with `[workspace]` — for backward compat with unmigrated workspaces.
///
/// If no workspace root is found, returns the nearest directory containing any
/// Cargo.toml, or `start` itself — callers should check `WorkspaceMap::is_workspace`.
pub fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut current = start.to_path_buf();
    let mut cargo_ws_root: Option<PathBuf> = None;
    let mut fallback: Option<PathBuf> = None;
    loop {
        // Prefer Polylith.toml — if found, this directory is the root.
        if current.join("Polylith.toml").exists() {
            return Ok(current);
        }
        let candidate = current.join("Cargo.toml");
        if candidate.exists() {
            let manifest = Manifest::from_path(&candidate)
                .with_context(|| format!("failed to parse {}", candidate.display()))?;
            if manifest.workspace.is_some() && cargo_ws_root.is_none() {
                cargo_ws_root = Some(current.clone());
            }
            if fallback.is_none() {
                fallback = Some(current.clone());
            }
        }
        if !current.pop() {
            // No Polylith.toml found; fall back to cargo workspace root, then any Cargo.toml
            return Ok(cargo_ws_root
                .or(fallback)
                .unwrap_or_else(|| start.to_path_buf()));
        }
    }
}

/// Build a `WorkspaceMap` from the given workspace root.
pub fn build_workspace_map(root: &Path) -> Result<WorkspaceMap> {
    let components = scan_bricks(root, BrickKind::Component)?;
    let bases = scan_bricks(root, BrickKind::Base)?;
    let projects = scan_projects(root)?;
    let (root_members, is_workspace, root_workspace_deps, root_workspace_interface_deps) = {
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
            // Also extract path deps from [workspace.dependencies] using toml_edit
            let ws_path_deps = {
                let content = fs::read_to_string(&toml_path).unwrap_or_default();
                let doc: toml_edit::DocumentMut = content.parse().unwrap_or_default();
                let mut map = std::collections::HashMap::new();
                if let Some(ws_deps) = doc
                    .get("workspace")
                    .and_then(|w| w.get("dependencies"))
                    .and_then(|d| d.as_table())
                {
                    for (key, val) in ws_deps.iter() {
                        let path = val
                            .as_value()
                            .and_then(|v| v.as_inline_table())
                            .and_then(|it| it.get("path"))
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                val.as_table()
                                    .and_then(|t| t.get("path"))
                                    .and_then(|v| v.as_value())
                                    .and_then(|v| v.as_str())
                            });
                        if let Some(path) = path {
                            let package = val
                                .as_value()
                                .and_then(|v| v.as_inline_table())
                                .and_then(|it| it.get("package"))
                                .and_then(|v| v.as_str())
                                .or_else(|| {
                                    val.as_table()
                                        .and_then(|t| t.get("package"))
                                        .and_then(|v| v.as_value())
                                        .and_then(|v| v.as_str())
                                })
                                .map(|s| s.to_string());
                            map.insert(key.to_string(), WorkspacePathDep {
                                path: path.to_string(),
                                package,
                            });
                        }
                    }
                }
                map
            };
            (members, is_ws, ws_deps, ws_path_deps)
        } else {
            (vec![], false, std::collections::HashMap::new(), std::collections::HashMap::new())
        }
    };
    // Parse Polylith.toml if present
    let polylith_toml = parse_polylith_toml(root)?;

    // When Polylith.toml is present, use its [libraries] as root_workspace_deps (if non-empty)
    let root_workspace_deps = if let Some(pt) = &polylith_toml {
        if !pt.libraries.is_empty() {
            pt.libraries.clone()
        } else {
            root_workspace_deps
        }
    } else {
        root_workspace_deps
    };

    let is_workspace = is_workspace || polylith_toml.is_some();

    Ok(WorkspaceMap {
        root: root.to_path_buf(),
        components,
        bases,
        projects,
        root_members,
        is_workspace,
        root_workspace_deps,
        root_workspace_interface_deps,
        polylith_toml,
    })
}

/// Read `Polylith.toml` from the given root directory, returning an error if not present.
///
/// Unlike `parse_polylith_toml`, this function requires the file to exist and returns
/// the `PolylithToml` directly (not wrapped in `Option`).
pub fn read_polylith_toml(root: &Path) -> Result<PolylithToml> {
    parse_polylith_toml(root)?
        .ok_or_else(|| anyhow::anyhow!("Polylith.toml not found at {}", root.display()))
}

/// Parse `Polylith.toml` from the given root directory, returning `None` if not present.
fn parse_polylith_toml(root: &Path) -> Result<Option<PolylithToml>> {
    let path = root.join("Polylith.toml");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let doc: toml_edit::DocumentMut = content
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;

    let schema_version = doc
        .get("workspace")
        .and_then(|w| w.get("schema_version"))
        .and_then(|v| v.as_value())
        .and_then(|v| v.as_integer())
        .map(|n| n as u32)
        .unwrap_or(1);

    let workspace_package = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .map(|pkg| WorkspacePackageMeta {
            version: pkg.get("version").and_then(|v| v.as_str()).map(|s| s.to_string()),
            edition: pkg.get("edition").and_then(|v| v.as_str()).map(|s| s.to_string()),
            authors: pkg
                .get("authors")
                .and_then(|v| v.as_value())
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default(),
            license: pkg.get("license").and_then(|v| v.as_str()).map(|s| s.to_string()),
            repository: pkg.get("repository").and_then(|v| v.as_str()).map(|s| s.to_string()),
        });

    let mut libraries = std::collections::HashMap::new();
    if let Some(libs) = doc.get("libraries").and_then(|t| t.as_table()) {
        for (k, v) in libs.iter() {
            // Skip path deps
            let has_path = v.as_value().and_then(|v| v.as_inline_table()).and_then(|it| it.get("path")).is_some()
                || v.as_table().and_then(|t| t.get("path")).is_some();
            if has_path {
                continue;
            }
            let version = parse_version_from_item(v);
            let features = parse_features_from_item(v);
            libraries.insert(k.to_string(), ExternalDepInfo { features, version });
        }
    }

    let mut profiles = std::collections::HashMap::new();
    if let Some(profs) = doc.get("profiles").and_then(|t| t.as_table()) {
        for (k, v) in profs.iter() {
            if let Some(s) = v.as_value().and_then(|v| v.as_str()) {
                profiles.insert(k.to_string(), s.to_string());
            }
        }
    }

    Ok(Some(PolylithToml {
        schema_version,
        workspace_package,
        libraries,
        profiles,
    }))
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
        // Use toml_edit for raw parsing — avoids workspace resolution which fails
        // when there is no [workspace] in the root (Polylith.toml workspaces).
        let doc = fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let deps = doc
            .get("dependencies")
            .and_then(|d| d.as_table())
            .map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default();
        let interface = doc
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("polylith"))
            .and_then(|p| p.get("interface"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Extract dep keys that use direct path deps (not workspace = true)
        let path_dep_keys = {
            let mut keys = vec![];
            if let Some(deps_table) = doc.get("dependencies").and_then(|d| d.as_table()) {
                for (k, v) in deps_table.iter() {
                    let has_path = v
                        .as_value()
                        .and_then(|v| v.as_inline_table())
                        .and_then(|it| it.get("path"))
                        .is_some()
                        || v.as_table()
                            .and_then(|t| t.get("path"))
                            .is_some();
                    let is_workspace = v
                        .as_value()
                        .and_then(|v| v.as_inline_table())
                        .and_then(|it| it.get("workspace"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        || v.as_table()
                            .and_then(|t| t.get("workspace"))
                            .and_then(|v| v.as_value())
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                    if has_path && !is_workspace {
                        keys.push(k.to_string());
                    }
                }
            }
            keys
        };
        bricks.push(Brick {
            name,
            kind: kind.clone(),
            path: path.clone(),
            deps,
            manifest_path,
            interface,
            path_dep_keys,
        });
    }
    bricks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(bricks)
}

fn parse_version_from_item(v: &toml_edit::Item) -> Option<String> {
    // bare string: `serde = "1.0"`
    if let Some(s) = v.as_value().and_then(|v| v.as_str()) {
        if s != "*" { return Some(s.to_string()); }
        return None;
    }
    // inline table
    let from_inline = v
        .as_value()
        .and_then(|v| v.as_inline_table())
        .and_then(|it| it.get("version"))
        .and_then(|v| v.as_str())
        .filter(|s| *s != "*")
        .map(|s| s.to_string());
    if from_inline.is_some() { return from_inline; }
    // regular table
    v.as_table()
        .and_then(|t| t.get("version"))
        .and_then(|v| v.as_value())
        .and_then(|v| v.as_str())
        .filter(|s| *s != "*")
        .map(|s| s.to_string())
}

fn parse_features_from_item(v: &toml_edit::Item) -> Vec<String> {
    let arr = v
        .as_value()
        .and_then(|v| v.as_inline_table())
        .and_then(|it| it.get("features"))
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

        let extract_version = |v: &toml_edit::Item| parse_version_from_item(v);
        let extract_features = |v: &toml_edit::Item| parse_features_from_item(v);

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
        let deps: Vec<String> = dep_set.into_iter().collect();
        let has_own_workspace = doc.get("workspace").is_some();
        // Extract the `name` field from the first `[[bin]]` entry, if present.
        let bin_name = doc
            .get("bin")
            .and_then(|b| b.as_array_of_tables())
            .and_then(|arr| arr.iter().next())
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_value())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        projects.push(Project {
            name,
            path: path.clone(),
            deps,
            has_own_workspace,
            bin_name,
            dep_paths,
            external_deps,
        });
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

/// Discover all profiles in `root/profiles/*.profile`.
/// Returns an empty vec if the `profiles/` directory doesn't exist.
pub fn discover_profiles(root: &Path) -> Result<Vec<Profile>> {
    use std::collections::HashMap;
    let profiles_dir = root.join("profiles");
    if !profiles_dir.exists() {
        return Ok(vec![]);
    }
    let mut profiles = vec![];
    for entry in std::fs::read_dir(&profiles_dir)
        .with_context(|| format!("reading {}", profiles_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("profile") {
            continue;
        }
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let doc: toml_edit::DocumentMut = content
            .parse()
            .with_context(|| format!("parsing {}", path.display()))?;
        // Parse [implementations]: each value is a string path
        let mut implementations = HashMap::new();
        if let Some(impls) = doc.get("implementations").and_then(|t| t.as_table()) {
            for (k, v) in impls.iter() {
                if let Some(s) = v.as_value().and_then(|v| v.as_str()) {
                    implementations.insert(k.to_string(), s.to_string());
                }
            }
        }
        // Parse [libraries]: same parsing as workspace deps (version + features)
        let mut libraries = HashMap::new();
        if let Some(libs) = doc.get("libraries").and_then(|t| t.as_table()) {
            for (k, v) in libs.iter() {
                let version = parse_version_from_item(v);
                let features = parse_features_from_item(v);
                libraries.insert(k.to_string(), ExternalDepInfo { features, version });
            }
        }
        profiles.push(Profile { name, path: path.clone(), implementations, libraries });
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

/// Compute the resolved data needed to generate a profile workspace Cargo.toml.
/// This is pure analysis — no file writes.
pub fn resolve_profile_workspace(
    root: &Path,
    profile: &super::model::Profile,
    map: &super::model::WorkspaceMap,
) -> super::model::ResolvedProfileWorkspace {
    use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
    use super::model::ResolvedProfileWorkspace;

    // Profile dir location (two levels below root): profiles/<name>/
    let profile_dir = root.join("profiles").join(&profile.name);

    // Helper: compute path relative to profile_dir, as a forward-slash string
    let rel_str = |abs: &Path| -> String {
        let from: Vec<_> = profile_dir.components().collect();
        let to: Vec<_> = abs.components().collect();
        let common = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();
        let up = from.len() - common;
        let mut rel = std::path::PathBuf::new();
        for _ in 0..up { rel.push(".."); }
        for part in &to[common..] { rel.push(part); }
        rel.to_string_lossy().replace('\\', "/")
    };

    // Build lookup maps from map.components
    // name_to_brick: package name -> Brick
    let name_to_brick: HashMap<&str, &super::model::Brick> = map
        .components
        .iter()
        .map(|b| (b.name.as_str(), b))
        .collect();

    // Build selected_impls: interface_key -> package_name (selected implementation)
    // Start with root_workspace_interface_deps (defaults), then override with profile.implementations
    let mut selected_impls: HashMap<String, String> = HashMap::new();

    // Add defaults from root_workspace_interface_deps
    for (iface_key, path_dep) in &map.root_workspace_interface_deps {
        // Resolve the path to a brick's package name
        let rel_path = path_dep.path.replace('\\', "/");
        if let Some(brick) = map.components.iter().find(|c| {
            c.path.strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                == Ok(rel_path.clone())
        }) {
            selected_impls.insert(iface_key.clone(), brick.name.clone());
        } else if let Some(pkg) = &path_dep.package {
            selected_impls.insert(iface_key.clone(), pkg.clone());
        }
    }

    // Override with profile.implementations
    for (iface_key, impl_rel_path) in &profile.implementations {
        let impl_rel = impl_rel_path.replace('\\', "/");
        if let Some(brick) = map.components.iter().find(|c| {
            c.path.strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                == Ok(impl_rel.clone())
        }) {
            selected_impls.insert(iface_key.clone(), brick.name.clone());
        }
    }

    // Define a helper closure resolve_dep_key(dep_key) -> Option<package_name>
    // Returns the selected package name for a dep key, or None if it's not a component.
    let resolve_dep_key = |dep_key: &str| -> Option<String> {
        // If dep_key is directly in selected_impls (interface key) -> return selected package name
        if let Some(pkg) = selected_impls.get(dep_key) {
            return Some(pkg.clone());
        }
        // If dep_key matches any brick's interface field -> look up selected_impls by that interface
        if let Some(brick) = map.components.iter().find(|c| {
            c.interface.as_deref() == Some(dep_key)
        }) {
            let iface = brick.interface.as_deref().unwrap_or(dep_key);
            if let Some(pkg) = selected_impls.get(iface) {
                return Some(pkg.clone());
            }
        }
        // If dep_key matches a component package name directly -> return it
        if name_to_brick.contains_key(dep_key) {
            return Some(dep_key.to_string());
        }
        // Otherwise -> None (external dep or base, skip)
        None
    };

    // BFS transitive closure to find all transitively required components
    let mut included_components: HashSet<String> = HashSet::new();
    let mut worklist: VecDeque<String> = VecDeque::new();

    // Seed worklist from bases' deps and projects' deps
    for base in &map.bases {
        for dep in &base.deps {
            if let Some(pkg) = resolve_dep_key(dep) {
                worklist.push_back(pkg);
            }
        }
    }
    for proj in &map.projects {
        for dep in &proj.deps {
            if let Some(pkg) = resolve_dep_key(dep) {
                worklist.push_back(pkg);
            }
        }
    }

    // BFS
    while let Some(pkg_name) = worklist.pop_front() {
        if included_components.contains(&pkg_name) {
            continue;
        }
        included_components.insert(pkg_name.clone());
        // Look up the brick and recurse into its deps
        if let Some(brick) = name_to_brick.get(pkg_name.as_str()) {
            for dep in &brick.deps {
                if let Some(dep_pkg) = resolve_dep_key(dep) {
                    if !included_components.contains(&dep_pkg) {
                        worklist.push_back(dep_pkg);
                    }
                }
            }
        }
    }

    // Members: only selected components + all bases + all projects
    let mut members = vec![];
    for brick in &map.components {
        if included_components.contains(&brick.name) {
            members.push(rel_str(&brick.path));
        }
    }
    for base in &map.bases {
        members.push(rel_str(&base.path));
    }
    for proj in &map.projects {
        members.push(rel_str(&proj.path));
    }

    // Interface (path) deps
    let mut iface_keys: BTreeSet<String> = map
        .root_workspace_interface_deps
        .keys()
        .cloned()
        .collect();
    for k in profile.implementations.keys() {
        iface_keys.insert(k.clone());
    }

    let mut interface_dep_lines = vec![];
    for key in &iface_keys {
        if let Some(impl_rel_path) = profile.implementations.get(key) {
            // Profile override: look up the component package name from workspace map
            let abs = root.join(impl_rel_path);
            let pkg_name = map
                .components
                .iter()
                .find(|c| {
                    // Compare by stripping root prefix and matching relative path
                    c.path.strip_prefix(root).map(|p| p.to_string_lossy().replace('\\', "/"))
                        == Ok(impl_rel_path.replace('\\', "/"))
                })
                .map(|c| c.name.clone())
                .unwrap_or_else(|| key.clone());
            let path_str = rel_str(&abs);
            if pkg_name == *key {
                interface_dep_lines.push(format!("{} = {{ path = \"{}\" }}", key, path_str));
            } else {
                interface_dep_lines.push(format!(
                    "{} = {{ path = \"{}\", package = \"{}\" }}",
                    key, path_str, pkg_name
                ));
            }
        } else if let Some(root_dep) = map.root_workspace_interface_deps.get(key) {
            let abs = root.join(&root_dep.path);
            let path_str = rel_str(&abs);
            if let Some(pkg) = &root_dep.package {
                interface_dep_lines.push(format!(
                    "{} = {{ path = \"{}\", package = \"{}\" }}",
                    key, path_str, pkg
                ));
            } else {
                interface_dep_lines.push(format!("{} = {{ path = \"{}\" }}", key, path_str));
            }
        }
    }

    // Library deps
    let mut lib_keys: BTreeSet<String> = map
        .root_workspace_deps
        .keys()
        .cloned()
        .collect();
    for k in profile.libraries.keys() {
        lib_keys.insert(k.clone());
    }

    let mut library_dep_lines = vec![];
    for key in &lib_keys {
        let info = if let Some(ov) = profile.libraries.get(key) {
            ov
        } else if let Some(ri) = map.root_workspace_deps.get(key) {
            ri
        } else {
            continue;
        };
        match (&info.version, info.features.is_empty()) {
            (Some(v), true) => library_dep_lines.push(format!("{} = \"{}\"", key, v)),
            (Some(v), false) => library_dep_lines.push(format!(
                "{} = {{ version = \"{}\", features = [{}] }}",
                key, v,
                info.features.iter().map(|f| format!("\"{}\"", f)).collect::<Vec<_>>().join(", ")
            )),
            (None, false) => library_dep_lines.push(format!(
                "{} = {{ features = [{}] }}",
                key,
                info.features.iter().map(|f| format!("\"{}\"", f)).collect::<Vec<_>>().join(", ")
            )),
            (None, true) => {} // no version, no features — skip
        }
    }

    let workspace_package = map
        .polylith_toml
        .as_ref()
        .and_then(|pt| pt.workspace_package.clone());

    ResolvedProfileWorkspace {
        profile_name: profile.name.clone(),
        members,
        interface_dep_lines,
        library_dep_lines,
        workspace_package,
    }
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
        assert!(map.projects.len() >= 2, "expected at least 2 projects, got {}", map.projects.len());
        assert!(map.projects.iter().any(|p| p.name == "main-project"), "main-project missing");
    }

    #[test]
    fn detects_project_with_own_workspace() {
        let map = build_workspace_map(&fixture()).unwrap();
        // The fixture's standalone-project no longer has its own [workspace] section.
        let standalone = map.projects.iter().find(|p| p.name == "standalone-project").unwrap();
        assert!(!standalone.has_own_workspace);
        let main = map.projects.iter().find(|p| p.name == "main-project").unwrap();
        assert!(!main.has_own_workspace);
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

    #[test]
    fn discovers_profiles_from_fixture() {
        let profiles = discover_profiles(&fixture()).unwrap();
        // fixture has profiles/dev.profile
        assert!(!profiles.is_empty(), "expected at least one profile");
        let dev = profiles.iter().find(|p| p.name == "dev");
        assert!(dev.is_some(), "expected dev profile");
    }

    #[test]
    fn discovers_no_profiles_when_dir_missing() {
        use tempfile::TempDir;
        use std::fs;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers=[]\nresolver=\"2\"\n",
        ).unwrap();
        let profiles = discover_profiles(dir.path()).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn resolve_profile_workspace_excludes_unselected_implementation() {
        use tempfile::TempDir;
        use std::fs;
        use std::collections::HashMap;
        use super::super::model::{Brick, BrickKind, Profile, Project, WorkspaceMap, WorkspacePathDep};

        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create minimal directory structure so rel_str works
        fs::create_dir_all(root.join("components/store_mem")).unwrap();
        fs::create_dir_all(root.join("components/store_file")).unwrap();
        fs::create_dir_all(root.join("bases/app")).unwrap();
        fs::create_dir_all(root.join("projects/myproject")).unwrap();

        // Two components implementing the same "store" interface
        let store_mem = Brick {
            name: "store_mem".to_string(),
            kind: BrickKind::Component,
            path: root.join("components/store_mem"),
            deps: vec![],
            manifest_path: root.join("components/store_mem/Cargo.toml"),
            interface: Some("store".to_string()),
            path_dep_keys: vec![],
        };
        let store_file = Brick {
            name: "store_file".to_string(),
            kind: BrickKind::Component,
            path: root.join("components/store_file"),
            deps: vec![],
            manifest_path: root.join("components/store_file/Cargo.toml"),
            interface: Some("store".to_string()),
            path_dep_keys: vec![],
        };
        // One base that depends on "store" (the interface key)
        let app_base = Brick {
            name: "app".to_string(),
            kind: BrickKind::Base,
            path: root.join("bases/app"),
            deps: vec!["store".to_string()],
            manifest_path: root.join("bases/app/Cargo.toml"),
            interface: None,
            path_dep_keys: vec![],
        };
        // One project
        let myproject = Project {
            name: "myproject".to_string(),
            path: root.join("projects/myproject"),
            deps: vec![],
            has_own_workspace: false,
            bin_name: None,
            dep_paths: vec![],
            external_deps: HashMap::new(),
        };

        // Root workspace interface dep points at store_mem (the default)
        let mut root_workspace_interface_deps = HashMap::new();
        root_workspace_interface_deps.insert("store".to_string(), WorkspacePathDep {
            path: "components/store_mem".to_string(),
            package: None,
        });

        let map = WorkspaceMap {
            root: root.to_path_buf(),
            components: vec![store_file.clone(), store_mem.clone()],
            bases: vec![app_base],
            projects: vec![myproject],
            root_members: vec![],
            is_workspace: true,
            root_workspace_deps: HashMap::new(),
            root_workspace_interface_deps,
            polylith_toml: None,
        };

        // Profile selects store_file for the "store" interface
        let mut implementations = HashMap::new();
        implementations.insert("store".to_string(), "components/store_file".to_string());
        let profile = Profile {
            name: "test".to_string(),
            path: root.join("profiles/test.profile"),
            implementations,
            libraries: HashMap::new(),
        };

        let resolved = resolve_profile_workspace(root, &profile, &map);

        // store_file should be included; store_mem should NOT be included
        let has_store_file = resolved.members.iter().any(|m| m.contains("store_file"));
        let has_store_mem = resolved.members.iter().any(|m| m.contains("store_mem"));

        assert!(has_store_file, "store_file should be in members: {:?}", resolved.members);
        assert!(!has_store_mem, "store_mem should NOT be in members: {:?}", resolved.members);
    }

    #[test]
    fn find_workspace_root_finds_polylith_toml() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();

        // Create Polylith.toml at root (no Cargo.toml with [workspace])
        fs::write(
            tmp.path().join("Polylith.toml"),
            "[workspace]\nschema_version = 1\n",
        ).unwrap();

        // Create a subdirectory
        let subdir = tmp.path().join("components/my-comp");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(
            subdir.join("Cargo.toml"),
            "[package]\nname = \"my-comp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ).unwrap();

        // find_workspace_root from the subdirectory should find the Polylith.toml directory
        let found = find_workspace_root(&subdir).unwrap();
        assert_eq!(found, tmp.path(), "should find Polylith.toml directory");
    }

    #[test]
    fn parse_polylith_toml_returns_none_when_absent() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let result = parse_polylith_toml(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_polylith_toml_parses_all_sections() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();

        fs::write(
            tmp.path().join("Polylith.toml"),
            r#"[workspace]
schema_version = 1

[workspace.package]
version = "0.5.0"
edition = "2021"
license = "MIT"

[libraries]
serde = { version = "1", features = ["derive"] }
tokio = "1.40"

[profiles]
dev = "profiles/dev.profile"
prod = "profiles/prod.profile"
"#,
        ).unwrap();

        let pt = parse_polylith_toml(tmp.path()).unwrap().unwrap();
        assert_eq!(pt.schema_version, 1);

        let pkg = pt.workspace_package.as_ref().unwrap();
        assert_eq!(pkg.version.as_deref(), Some("0.5.0"));
        assert_eq!(pkg.edition.as_deref(), Some("2021"));
        assert_eq!(pkg.license.as_deref(), Some("MIT"));

        assert_eq!(pt.libraries.len(), 2);
        let serde_info = pt.libraries.get("serde").unwrap();
        assert_eq!(serde_info.version.as_deref(), Some("1"));
        assert!(serde_info.features.contains(&"derive".to_string()));

        assert_eq!(pt.profiles.len(), 2);
        assert_eq!(pt.profiles.get("dev").map(|s| s.as_str()), Some("profiles/dev.profile"));
    }
}
