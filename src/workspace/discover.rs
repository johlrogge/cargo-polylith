use std::fs;
use std::path::{Path, PathBuf};

use cargo_toml::Manifest;

use super::error::WorkspaceError;
use super::model::{Brick, BrickKind, ExternalDepInfo, PolylithToml, Profile, Project, RootDemotionPlan, VersioningPolicy, WorkspaceMap, WorkspacePackageMeta, WorkspacePathDep};

type Result<T> = std::result::Result<T, WorkspaceError>;

/// Helper: map an `std::io::Error` to `WorkspaceError::Io` with the given path.
fn io_err(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> WorkspaceError {
    let p = path.into();
    move |e| WorkspaceError::Io { path: p, source: e }
}

/// Helper: map a parse error to `WorkspaceError::TomlParse` with the given path.
fn parse_err<E: std::error::Error + Send + Sync + 'static>(path: impl Into<PathBuf>) -> impl FnOnce(E) -> WorkspaceError {
    let p = path.into();
    move |e| WorkspaceError::TomlParse { path: p, source: Box::new(e) }
}

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
            if !abs.join("Polylith.toml").exists() && !abs.join("Cargo.toml").exists() {
                return Err(WorkspaceError::Other(format!(
                    "no Polylith.toml or Cargo.toml found at workspace root '{}'",
                    abs.display()
                )));
            }
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
                .map_err(parse_err(&candidate))?;
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
                .map_err(parse_err(&toml_path))?;
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
                            Some((key, ExternalDepInfo { features, version, raw: None }))
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
                        if let Some(path) = toml_str(val, "path") {
                            let package = toml_str(val, "package");
                            map.insert(key.to_string(), WorkspacePathDep { path, package });
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
    // Read package metadata from root Cargo.toml [package]
    let root_package_meta = read_root_package_meta(root)?;

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

    let mut map = WorkspaceMap {
        root: root.to_path_buf(),
        components,
        bases,
        projects,
        root_members,
        is_workspace,
        root_workspace_deps,
        root_workspace_interface_deps,
        polylith_toml,
        root_package_meta,
        component_by_name: std::collections::HashMap::new(),
        component_by_interface: std::collections::HashMap::new(),
        base_by_name: std::collections::HashMap::new(),
    };
    map.component_by_name = map.components.iter().enumerate()
        .map(|(i, c)| (c.name.clone(), i))
        .collect();
    map.component_by_interface = map.components.iter().enumerate()
        .flat_map(|(i, c)| c.interface.iter().map(move |iface| (iface.clone(), i)))
        .collect();
    map.base_by_name = map.bases.iter().enumerate()
        .map(|(i, b)| (b.name.clone(), i))
        .collect();
    Ok(map)
}

/// Read package metadata from root `Cargo.toml` `[package]` section.
pub fn read_root_package_meta(root: &Path) -> Result<Option<WorkspacePackageMeta>> {
    let toml_path = root.join("Cargo.toml");
    if !toml_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&toml_path).map_err(io_err(&toml_path))?;
    let doc: toml_edit::DocumentMut = content.parse().map_err(parse_err(&toml_path))?;

    // Prefer [package] for backward compatibility with the legacy demotion model.
    // Fall back to [workspace.package] for workspaces that have not been demoted
    // (pre-migration root Cargo.toml) or for profile-generated root Cargo.toml files
    // that use [workspace.package] to carry metadata.
    let pkg = if let Some(p) = doc.get("package") {
        p
    } else if let Some(wp) = doc.get("workspace").and_then(|w| w.get("package")) {
        wp
    } else {
        return Ok(None);
    };

    let version = pkg.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
    let edition = pkg.get("edition").and_then(|v| v.as_str()).map(|s| s.to_string());
    let authors: Vec<String> = pkg
        .get("authors")
        .and_then(|v| v.as_value())
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let license = pkg.get("license").and_then(|v| v.as_str()).map(|s| s.to_string());
    let repository = pkg.get("repository").and_then(|v| v.as_str()).map(|s| s.to_string());

    let has_meta = version.is_some() || edition.is_some() || !authors.is_empty()
        || license.is_some() || repository.is_some();

    if has_meta {
        Ok(Some(WorkspacePackageMeta { version, edition, authors, license, repository }))
    } else {
        Ok(None)
    }
}

/// Read `Polylith.toml` from the given root directory, returning an error if not present.
///
/// Unlike `parse_polylith_toml`, this function requires the file to exist and returns
/// the `PolylithToml` directly (not wrapped in `Option`).
pub fn read_polylith_toml(root: &Path) -> Result<PolylithToml> {
    parse_polylith_toml(root)?
        .ok_or_else(|| WorkspaceError::Other(format!(
            "Polylith.toml not found at {}",
            root.display()
        )))
}

/// Collect only the path deps from `[workspace.dependencies]` in the root `Cargo.toml`.
///
/// This is a targeted, cheap alternative to `build_workspace_map` for the pre-migration
/// phase of `profile migrate`. At that point, `Polylith.toml` does not yet exist, and we
/// only need the interface wiring diagram (path deps) to write `profiles/dev.profile` and
/// to strip workspace inheritance from bricks. Building the full `WorkspaceMap` would scan
/// all components/bases/projects unnecessarily.
///
/// Returns a map of dep key → `WorkspacePathDep` for every path dep found in
/// `[workspace.dependencies]`.
pub fn collect_root_interface_deps(root: &Path) -> Result<std::collections::HashMap<String, WorkspacePathDep>> {
    let toml_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&toml_path).map_err(io_err(&toml_path))?;
    let doc: toml_edit::DocumentMut = content.parse().map_err(parse_err(&toml_path))?;
    let mut map = std::collections::HashMap::new();
    if let Some(ws_deps) = doc
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (key, val) in ws_deps.iter() {
            if let Some(path) = toml_str(val, "path") {
                let package = toml_str(val, "package");
                map.insert(key.to_string(), WorkspacePathDep { path, package });
            }
        }
    }
    Ok(map)
}

/// Analyse the root workspace and produce a `RootDemotionPlan` — pure read, no writes.
///
/// Reads root `Cargo.toml` to extract `[workspace.package]` metadata and non-path
/// `[workspace.dependencies]`, then scans `profiles/*.profile` for profile names.
/// Returns a plan that `scaffold::write_polylith_toml` can consume.
pub fn plan_root_demotion(root: &Path) -> Result<RootDemotionPlan> {
    use std::collections::HashMap;

    let manifest_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path).map_err(io_err(&manifest_path))?;
    let doc: toml_edit::DocumentMut = content.parse()
        .map_err(parse_err(&manifest_path))?;

    // Extract [workspace.package] fields
    let version = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let edition = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("edition"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let authors: Vec<String> = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("authors"))
        .and_then(|v| v.as_value())
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let license = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("license"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let repository = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("repository"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let has_package_meta = version.is_some() || edition.is_some() || !authors.is_empty()
        || license.is_some() || repository.is_some();

    let workspace_package = if has_package_meta {
        Some(WorkspacePackageMeta { version, edition, authors, license, repository })
    } else {
        None
    };

    // Extract non-path deps from [workspace.dependencies]
    let mut libraries: HashMap<String, ExternalDepInfo> = HashMap::new();
    if let Some(ws_deps) = doc
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (key, val) in ws_deps.iter() {
            // Skip path deps — they are interface wiring, not libraries
            let has_path = val
                .as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|it| it.get("path"))
                .is_some()
                || val.as_table().and_then(|t| t.get("path")).is_some();
            if has_path {
                continue;
            }
            let version = parse_version_from_item(val);
            let mut features = parse_features_from_item(val);
            features.sort();
            let raw = Some(render_dep_item_raw(val));
            libraries.insert(key.to_string(), ExternalDepInfo { version, features, raw });
        }
    }

    // Discover existing profile names from profiles/*.profile
    let profiles_dir = root.join("profiles");
    let mut profiles: HashMap<String, String> = HashMap::new();
    if profiles_dir.exists() {
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("profile") {
                    continue;
                }
                if let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                    profiles.insert(
                        name.clone(),
                        format!("profiles/{}.profile", name),
                    );
                }
            }
        }
    }

    Ok(RootDemotionPlan { workspace_package, libraries, profiles })
}

/// Parse `Polylith.toml` from the given root directory, returning `None` if not present.
fn parse_polylith_toml(root: &Path) -> Result<Option<PolylithToml>> {
    let path = root.join("Polylith.toml");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(io_err(&path))?;
    let doc: toml_edit::DocumentMut = content.parse().map_err(parse_err(&path))?;

    let schema_version = doc
        .get("workspace")
        .and_then(|w| w.get("schema_version"))
        .and_then(|v| v.as_value())
        .and_then(|v| v.as_integer())
        .map(|n| n as u32)
        .unwrap_or(1);

    let mut libraries = std::collections::HashMap::new();
    if let Some(libs) = doc.get("libraries").and_then(|t| t.as_table()) {
        for (k, v) in libs.iter() {
            // Skip path deps
            if toml_str(v, "path").is_some() {
                continue;
            }
            let version = parse_version_from_item(v);
            let features = parse_features_from_item(v);
            let raw = Some(render_dep_item_raw(v));
            libraries.insert(k.to_string(), ExternalDepInfo { features, version, raw });
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

    // Parse [versioning] section — missing section means legacy workspace (both fields None).
    let (versioning_policy, workspace_version, tag_prefix) = if let Some(ver) = doc.get("versioning").and_then(|t| t.as_table()) {
        let policy = if let Some(policy_str) = ver.get("policy").and_then(|v| v.as_value()).and_then(|v| v.as_str()) {
            let p = match policy_str {
                "relaxed" => VersioningPolicy::Relaxed,
                "strict" => VersioningPolicy::Strict,
                other => return Err(WorkspaceError::Other(format!(
                    "unknown versioning policy '{}' in Polylith.toml (expected 'relaxed' or 'strict')",
                    other
                ))),
            };
            Some(p)
        } else {
            None
        };
        let version = ver.get("version").and_then(|v| v.as_value()).and_then(|v| v.as_str()).map(|s| s.to_string());
        let tag_prefix = ver.get("tag_prefix").and_then(|v| v.as_value()).and_then(|v| v.as_str()).map(|s| s.to_string());
        (policy, version, tag_prefix)
    } else {
        (None, None, None)
    };

    Ok(Some(PolylithToml {
        schema_version,
        libraries,
        profiles,
        versioning_policy,
        workspace_version,
        tag_prefix,
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
    for entry in std::fs::read_dir(&dir).map_err(io_err(&dir))? {
        let entry = entry.map_err(io_err(&dir))?;
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
            .map_err(io_err(&manifest_path))?
            .parse::<toml_edit::DocumentMut>()
            .map_err(parse_err(&manifest_path))?;
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
                    let has_path = toml_str(v, "path").is_some();
                    let is_workspace = toml_bool(v, "workspace").unwrap_or(false);
                    if has_path && !is_workspace {
                        keys.push(k.to_string());
                    }
                }
            }
            keys
        };
        // Extract deps where package = "X" and X differs from the dep key
        let hardwired_pkg_deps: Vec<(String, String)> = {
            let mut hpd = vec![];
            for table_name in &["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(deps) = doc.get(table_name).and_then(|d| d.as_table()) {
                    for (key, val) in deps.iter() {
                        if let Some(pkg) = toml_str(val, "package") {
                            let normalized_key = key.replace('-', "_");
                            let normalized_pkg = pkg.replace('-', "_");
                            if normalized_key != normalized_pkg {
                                hpd.push((key.to_string(), pkg));
                            }
                        }
                    }
                }
            }
            hpd
        };
        bricks.push(Brick {
            name,
            kind,
            path: path.clone(),
            deps,
            manifest_path,
            interface,
            path_dep_keys,
            hardwired_pkg_deps,
        });
    }
    bricks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(bricks)
}

/// Extract a string value from a TOML item by key, handling both inline tables
/// (`foo = { key = "val" }`) and regular tables (`[dep]\nkey = "val"`).
fn toml_str(item: &toml_edit::Item, key: &str) -> Option<String> {
    item.as_value()
        .and_then(|v| v.as_inline_table())
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_str())
        .or_else(|| {
            item.as_table()
                .and_then(|t| t.get(key))
                .and_then(|i| i.as_value())
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string())
}

use crate::toml_utils::toml_bool;

fn parse_version_from_item(v: &toml_edit::Item) -> Option<String> {
    // bare string: `serde = "1.0"`
    if let Some(s) = v.as_value().and_then(|v| v.as_str()) {
        if s != "*" { return Some(s.to_string()); }
        return None;
    }
    toml_str(v, "version").filter(|s| s != "*")
}

/// Render a toml_edit Item as a raw TOML value string suitable for use as a dep spec.
fn render_dep_item_raw(v: &toml_edit::Item) -> String {
    if let Some(s) = v.as_value().and_then(|v| v.as_str()) {
        return format!("\"{}\"", s);
    }
    v.to_string().trim().to_string()
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
    for entry in std::fs::read_dir(&dir).map_err(io_err(&dir))? {
        let entry = entry.map_err(io_err(&dir))?;
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
            toml_str(v, "package").unwrap_or_else(|| k.to_string())
        };
        // Helper: extract the `path = "..."` value from a dep item (inline table or regular table).
        let extract_path = |v: &toml_edit::Item| -> Option<String> {
            toml_str(v, "path")
        };
        // Helper: check whether a dep item has an explicit `package = "..."` alias.
        let has_package_alias = |v: &toml_edit::Item| -> bool {
            toml_str(v, "package").is_some()
        };
        let mut dep_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut dep_paths: Vec<(String, PathBuf)> = vec![];
        let mut external_deps: std::collections::HashMap<String, ExternalDepInfo> =
            std::collections::HashMap::new();

        // Helper: check whether a dep item has `workspace = true`.
        let is_workspace_dep = |v: &toml_edit::Item| -> bool {
            toml_bool(v, "workspace").unwrap_or(false)
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
                        external_deps.insert(k.to_string(), ExternalDepInfo { features, version, raw: None });
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
        // Extract deps where package = "X" and X differs from the dep key
        let hardwired_pkg_deps: Vec<(String, String)> = {
            let mut hpd = vec![];
            for table_name in &["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(deps) = doc.get(table_name).and_then(|d| d.as_table()) {
                    for (key, val) in deps.iter() {
                        if let Some(pkg) = toml_str(val, "package") {
                            let normalized_key = key.replace('-', "_");
                            let normalized_pkg = pkg.replace('-', "_");
                            if normalized_key != normalized_pkg {
                                hpd.push((key.to_string(), pkg));
                            }
                        }
                    }
                }
            }
            hpd
        };
        projects.push(Project {
            name,
            path: path.clone(),
            deps,
            has_own_workspace,
            bin_name,
            dep_paths,
            external_deps,
            hardwired_pkg_deps,
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
    for entry in std::fs::read_dir(&profiles_dir).map_err(io_err(&profiles_dir))? {
        let entry = entry.map_err(io_err(&profiles_dir))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("profile") {
            continue;
        }
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let content = fs::read_to_string(&path).map_err(io_err(&path))?;
        let doc: toml_edit::DocumentMut = content.parse().map_err(parse_err(&path))?;
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
                libraries.insert(k.to_string(), ExternalDepInfo { features, version, raw: None });
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

    // Helper: compute path relative to root, as a forward-slash string.
    // Member paths are root-relative (e.g. "components/foo", "bases/bar") since the
    // generated Cargo.toml lives at the workspace root.
    let rel_str = |abs: &Path| -> String {
        abs.strip_prefix(root)
            .unwrap_or(abs)
            .to_string_lossy()
            .replace('\\', "/")
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

    let workspace_package = map.root_package_meta.clone();

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
            hardwired_pkg_deps: vec![],
        };
        let store_file = Brick {
            name: "store_file".to_string(),
            kind: BrickKind::Component,
            path: root.join("components/store_file"),
            deps: vec![],
            manifest_path: root.join("components/store_file/Cargo.toml"),
            interface: Some("store".to_string()),
            path_dep_keys: vec![],
            hardwired_pkg_deps: vec![],
        };
        let app_base = Brick {
            name: "app".to_string(),
            kind: BrickKind::Base,
            path: root.join("bases/app"),
            deps: vec!["store".to_string()],
            manifest_path: root.join("bases/app/Cargo.toml"),
            interface: None,
            path_dep_keys: vec![],
            hardwired_pkg_deps: vec![],
        };

        let mut root_workspace_interface_deps = HashMap::new();
        root_workspace_interface_deps.insert(
            "store".to_string(),
            WorkspacePathDep { path: "components/store_mem".to_string(), package: None },
        );

        let map = WorkspaceMap {
            root: root.to_path_buf(),
            components: vec![store_file.clone(), store_mem.clone()],
            bases: vec![app_base],
            projects: vec![Project {
                name: "myproject".to_string(),
                path: root.join("projects/myproject"),
                deps: vec![],
                has_own_workspace: false,
                bin_name: None,
                dep_paths: vec![],
                external_deps: HashMap::new(),
                hardwired_pkg_deps: vec![],
            }],
            root_members: vec![],
            is_workspace: true,
            root_workspace_deps: HashMap::new(),
            root_workspace_interface_deps,
            polylith_toml: None,
            root_package_meta: None,
            component_by_name: HashMap::new(),
            component_by_interface: HashMap::new(),
            base_by_name: HashMap::new(),
        };

        // Profile overrides to store_file
        let mut implementations = HashMap::new();
        implementations.insert("store".to_string(), "components/store_file".to_string());
        let profile = Profile {
            name: "file".to_string(),
            path: root.join("profiles/file.profile"),
            implementations,
            libraries: HashMap::new(),
        };

        let resolved = resolve_profile_workspace(root, &profile, &map);
        // store_file should be in members; store_mem should NOT
        assert!(resolved.members.iter().any(|m| m.contains("store_file")), "store_file missing from members");
        assert!(!resolved.members.iter().any(|m| m.contains("store_mem")), "store_mem should be excluded");
    }

    #[test]
    fn workspace_map_indexes_are_populated() {
        let map = build_workspace_map(&fixture()).unwrap();
        // component_by_name index: every component should be reachable by name
        for (i, comp) in map.components.iter().enumerate() {
            let &idx = map.component_by_name.get(&comp.name)
                .unwrap_or_else(|| panic!("component '{}' missing from component_by_name", comp.name));
            assert_eq!(idx, i);
        }
        // base_by_name index: every base should be reachable by name
        for (i, base) in map.bases.iter().enumerate() {
            let &idx = map.base_by_name.get(&base.name)
                .unwrap_or_else(|| panic!("base '{}' missing from base_by_name", base.name));
            assert_eq!(idx, i);
        }
        // component_by_interface index: every component with an interface should be reachable
        for (i, comp) in map.components.iter().enumerate() {
            if let Some(iface) = &comp.interface {
                let &idx = map.component_by_interface.get(iface)
                    .unwrap_or_else(|| panic!("interface '{}' missing from component_by_interface", iface));
                assert_eq!(idx, i);
            }
        }
    }

    #[test]
    fn classify_dep_uses_indexes() {
        use super::super::{classify_dep, DepKind};
        let map = build_workspace_map(&fixture()).unwrap();
        // "logger" and "parser" are components in the fixture
        assert_eq!(classify_dep("logger", &map), DepKind::Interface("logger"));
        assert_eq!(classify_dep("parser", &map), DepKind::Interface("parser"));
        // "cli" is a base
        assert_eq!(classify_dep("cli", &map), DepKind::Base("cli"));
        // unknown name
        assert_eq!(classify_dep("nonexistent_crate", &map), DepKind::External);
    }

    // --- Versioning section tests ---

    /// Missing [versioning] section → both fields None (backward compatible).
    #[test]
    fn parse_polylith_toml_no_versioning_section() {
        use tempfile::TempDir;
        use std::fs;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Polylith.toml"),
            "[workspace]\nschema_version = 1\n",
        ).unwrap();
        let result = parse_polylith_toml(dir.path()).unwrap().unwrap();
        assert!(result.versioning_policy.is_none());
        assert!(result.workspace_version.is_none());
    }

    /// [versioning] with policy = "relaxed" parses correctly.
    #[test]
    fn parse_polylith_toml_relaxed_policy() {
        use tempfile::TempDir;
        use std::fs;
        use super::super::model::VersioningPolicy;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Polylith.toml"),
            "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"1.0.0\"\n",
        ).unwrap();
        let result = parse_polylith_toml(dir.path()).unwrap().unwrap();
        assert_eq!(result.versioning_policy, Some(VersioningPolicy::Relaxed));
        assert_eq!(result.workspace_version.as_deref(), Some("1.0.0"));
    }

    /// [versioning] with policy = "strict" parses correctly.
    #[test]
    fn parse_polylith_toml_strict_policy() {
        use tempfile::TempDir;
        use std::fs;
        use super::super::model::VersioningPolicy;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Polylith.toml"),
            "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"strict\"\nversion = \"2.3.4\"\n",
        ).unwrap();
        let result = parse_polylith_toml(dir.path()).unwrap().unwrap();
        assert_eq!(result.versioning_policy, Some(VersioningPolicy::Strict));
        assert_eq!(result.workspace_version.as_deref(), Some("2.3.4"));
    }

    /// Unknown policy string returns an error.
    #[test]
    fn parse_polylith_toml_unknown_policy() {
        use tempfile::TempDir;
        use std::fs;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Polylith.toml"),
            "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"experimental\"\n",
        ).unwrap();
        let result = parse_polylith_toml(dir.path());
        assert!(result.is_err(), "expected error for unknown policy");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("experimental"), "error should mention the unknown value: {err_msg}");
    }

    /// [versioning] with version but no policy: version is Some, policy is None.
    #[test]
    fn parse_polylith_toml_version_without_policy() {
        use tempfile::TempDir;
        use std::fs;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Polylith.toml"),
            "[workspace]\nschema_version = 1\n\n[versioning]\nversion = \"0.5.0\"\n",
        ).unwrap();
        let result = parse_polylith_toml(dir.path()).unwrap().unwrap();
        assert!(result.versioning_policy.is_none());
        assert_eq!(result.workspace_version.as_deref(), Some("0.5.0"));
    }
}
