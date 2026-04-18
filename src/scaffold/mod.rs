pub mod error;
pub mod templates;

use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::DocumentMut;

use crate::workspace::{ResolvedProfileWorkspace, RootDemotionPlan};

use templates::*;

pub use error::ScaffoldError;

type Result<T> = std::result::Result<T, ScaffoldError>;

/// Helper: map an `std::io::Error` to `ScaffoldError::Io` with the given path.
fn io_err(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> ScaffoldError {
    let p = path.into();
    move |e| ScaffoldError::Io { path: p, source: e }
}

/// Helper: map a parse error to `ScaffoldError::TomlEdit` with the given path.
fn toml_err<E: std::error::Error + Send + Sync + 'static>(path: impl Into<PathBuf>) -> impl FnOnce(E) -> ScaffoldError {
    let p = path.into();
    move |e| ScaffoldError::TomlEdit { path: p, source: Box::new(e) }
}

/// Which polylith brick directory a dep entry lives in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrickKind {
    Component,
    Base,
}

/// Minimal description of one row (component or base) needed to update a
/// project's `[dependencies]`. Passed to [`write_project_deps`] by the TUI
/// rather than reaching directly into TUI-layer types.
#[derive(Debug, Clone)]
pub struct DepEntry {
    pub name: String,
    pub interface: Option<String>,
    pub kind: BrickKind,
    pub path: PathBuf,
    /// Whether this entry should be a direct dependency of the project.
    pub selected: bool,
}

/// Update `[dependencies]` in the project's Cargo.toml: add path deps for
/// direct-dep rows, remove brick path deps for deselected rows, leave external
/// deps untouched.
pub fn write_project_deps(project_path: &Path, entries: &[DepEntry]) -> Result<()> {
    let manifest_path = project_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path).map_err(io_err(&manifest_path))?;
    let mut doc: DocumentMut = content.parse().map_err(toml_err(&manifest_path))?;

    if doc.get("dependencies").is_none() {
        doc["dependencies"] = toml_edit::table();
    }
    let deps = doc["dependencies"]
        .as_table_mut()
        .ok_or_else(|| ScaffoldError::Other("[dependencies] is not a table".to_string()))?;

    for entry in entries {
        // Use the polylith interface name as the dep key when it differs from the
        // crate name — this enables substitution (e.g. stub vs real) without
        // changing call-site code. Cargo's `package` key handles the rename.
        let dep_key = entry
            .interface
            .as_deref()
            .filter(|iface| *iface != entry.name.as_str())
            .unwrap_or(&entry.name);

        if entry.selected {
            if deps.get(dep_key).is_none() {
                let kind_dir = match entry.kind {
                    BrickKind::Component => "components",
                    BrickKind::Base => "bases",
                };
                let dir_name = entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let path_str = format!("../../{}/{}", kind_dir, dir_name);
                let mut tbl = toml_edit::InlineTable::new();
                tbl.insert("path", toml_edit::Value::from(path_str));
                if dep_key != entry.name.as_str() {
                    tbl.insert("package", toml_edit::Value::from(entry.name.clone()));
                }
                deps[dep_key] =
                    toml_edit::Item::Value(toml_edit::Value::InlineTable(tbl));
            }
        } else if is_brick_dep(deps, dep_key) {
            deps.remove(dep_key);
        }
    }

    fs::write(&manifest_path, doc.to_string()).map_err(io_err(&manifest_path))?;
    Ok(())
}

/// Returns true if the dep entry has a `path` value pointing into components/ or bases/.
fn is_brick_dep(deps: &toml_edit::Table, name: &str) -> bool {
    let path_str = deps
        .get(name)
        .and_then(|item| {
            item.as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|t| t.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned())
                .or_else(|| {
                    item.as_table()
                        .and_then(|t| t.get("path"))
                        .and_then(|v| v.as_value())
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned())
                })
        });
    path_str
        .as_deref()
        .map(|p| p.contains("/components/") || p.contains("/bases/"))
        .unwrap_or(false)
}

/// Create the three polylith top-level directories and `.cargo/config.toml`.
pub fn init_workspace(root: &Path) -> Result<Vec<String>> {
    let mut warnings = vec![];
    for dir in &["components", "bases", "projects"] {
        let p = root.join(dir);
        if p.exists() {
            warnings.push(format!("'{}' already exists, skipping", dir));
        } else {
            fs::create_dir_all(&p).map_err(io_err(&p))?;
        }
    }
    let cargo_dir = root.join(".cargo");
    fs::create_dir_all(&cargo_dir).map_err(io_err(&cargo_dir))?;
    let config_path = cargo_dir.join("config.toml");
    if !config_path.exists() {
        fs::write(&config_path, cargo_config_toml()).map_err(io_err(&config_path))?;
    }
    // Write Polylith.toml with initial versioning configuration.
    let polylith_toml_path = root.join("Polylith.toml");
    if !polylith_toml_path.exists() {
        fs::write(&polylith_toml_path, polylith_toml_initial()).map_err(io_err(&polylith_toml_path))?;
    }
    Ok(warnings)
}

/// Create a new component under `<root>/components/<name>/`.
pub fn create_component(root: &Path, name: &str, interface: &str) -> Result<()> {
    let dir = root.join("components").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src).map_err(io_err(&src))?;

    let cargo_toml_path = dir.join("Cargo.toml");
    fs::write(&cargo_toml_path, component_cargo_toml(name, interface))
        .map_err(io_err(&cargo_toml_path))?;
    let lib_rs_path = src.join("lib.rs");
    fs::write(&lib_rs_path, component_lib_rs(name)).map_err(io_err(&lib_rs_path))?;
    let impl_path = src.join(format!("{name}.rs"));
    fs::write(&impl_path, component_impl_rs()).map_err(io_err(&impl_path))?;

    add_workspace_member(root, &format!("components/{name}"))?;
    Ok(())
}

/// Write or update the `[package.metadata.polylith] interface` key in a component's
/// `Cargo.toml`. Creates the metadata tables if they don't exist.
pub fn write_interface_to_toml(component_path: &Path, interface: &str) -> Result<()> {
    let manifest_path = component_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path).map_err(io_err(&manifest_path))?;
    let mut doc: DocumentMut = content.parse().map_err(toml_err(&manifest_path))?;
    if doc["package"].get("metadata").is_none() {
        doc["package"]["metadata"] = toml_edit::table();
    }
    if doc["package"]["metadata"].get("polylith").is_none() {
        doc["package"]["metadata"]["polylith"] = toml_edit::table();
    }
    doc["package"]["metadata"]["polylith"]["interface"] = toml_edit::value(interface);
    fs::write(&manifest_path, doc.to_string()).map_err(io_err(&manifest_path))?;
    Ok(())
}

/// Write or update the `[package.metadata.polylith] test-base` key in a base's `Cargo.toml`.
/// Creates the metadata tables if they don't exist.
pub fn write_test_base_to_toml(base_path: &Path, test_base: bool) -> Result<()> {
    let manifest_path = base_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path).map_err(io_err(&manifest_path))?;
    let mut doc: DocumentMut = content.parse().map_err(toml_err(&manifest_path))?;
    if doc["package"].get("metadata").is_none() {
        doc["package"]["metadata"] = toml_edit::table();
    }
    if doc["package"]["metadata"].get("polylith").is_none() {
        doc["package"]["metadata"]["polylith"] = toml_edit::table();
    }
    doc["package"]["metadata"]["polylith"]["test-base"] = toml_edit::value(test_base);
    fs::write(&manifest_path, doc.to_string()).map_err(io_err(&manifest_path))?;
    Ok(())
}

/// Create a new base under `<root>/bases/<name>/`.
pub fn create_base(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("bases").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src).map_err(io_err(&src))?;

    let cargo_toml_path = dir.join("Cargo.toml");
    fs::write(&cargo_toml_path, base_cargo_toml(name)).map_err(io_err(&cargo_toml_path))?;
    let lib_rs_path = src.join("lib.rs");
    fs::write(&lib_rs_path, base_lib_rs()).map_err(io_err(&lib_rs_path))?;

    add_workspace_member(root, &format!("bases/{name}"))?;
    Ok(())
}

/// Create a new project under `<root>/projects/<name>/`.
pub fn create_project(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("projects").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src).map_err(io_err(&src))?;

    let cargo_toml_path = dir.join("Cargo.toml");
    let project_toml_content = format!("{}{}", GENERATED_HEADER, project_cargo_toml(name));
    fs::write(&cargo_toml_path, project_toml_content).map_err(io_err(&cargo_toml_path))?;
    let main_rs_path = src.join("main.rs");
    fs::write(&main_rs_path, "fn main() {}\n").map_err(io_err(&main_rs_path))?;

    add_workspace_member(root, &format!("projects/{name}"))?;
    Ok(())
}

/// Write a root workspace Cargo.toml from pre-resolved profile data.
///
/// Writes to `<root>/Cargo.toml` directly — no profile subdirectory, no symlinks.
/// The generated file includes the `GENERATED_HEADER`, the profile name as a source
/// comment, `[workspace]` with members, `resolver = "2"`, and optionally
/// `[workspace.package]` and `[workspace.dependencies]`.
///
/// Member paths are root-relative (e.g. `components/foo`) since the file lives at
/// the workspace root — `resolve_profile_workspace` already produces this format.
///
/// Returns the path to the generated file.
pub fn write_root_workspace_from_profile(
    root: &Path,
    resolved: &ResolvedProfileWorkspace,
) -> Result<std::path::PathBuf> {
    let out_path = root.join("Cargo.toml");

    let member_lines = resolved
        .members
        .iter()
        .map(|m| format!("    \"{}\"", m))
        .collect::<Vec<_>>()
        .join(",\n");

    let mut dep_lines = resolved.interface_dep_lines.clone();
    dep_lines.extend(resolved.library_dep_lines.iter().cloned());

    // Build [workspace.package] section if workspace_package is set
    let pkg_section = if let Some(pkg) = &resolved.workspace_package {
        let mut lines = vec!["[workspace.package]".to_string()];
        if let Some(v) = &pkg.version {
            lines.push(format!("version = \"{}\"", v));
        }
        if let Some(e) = &pkg.edition {
            lines.push(format!("edition = \"{}\"", e));
        }
        if let Some(authors) = &pkg.authors {
            // Note: profile generation suppresses authors = [] (less noise in generated output).
            // Contrast with migrate, which preserves explicit empty arrays for mirror-fidelity
            // from the source. This asymmetry is intentional — do not "fix" it to be symmetric.
            if !authors.is_empty() {
                let authors_list = authors
                    .iter()
                    .map(|a| format!("\"{}\"", a))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("authors = [{}]", authors_list));
            }
        }
        if let Some(l) = &pkg.license {
            lines.push(format!("license = \"{}\"", l));
        }
        if let Some(r) = &pkg.repository {
            lines.push(format!("repository = \"{}\"", r));
        }
        format!("\n{}\n", lines.join("\n"))
    } else {
        String::new()
    };

    let deps_section = if dep_lines.is_empty() {
        String::new()
    } else {
        format!("\n[workspace.dependencies]\n{}\n", dep_lines.join("\n"))
    };

    let profiles_section = if resolved.cargo_profile_sections.is_empty() {
        String::new()
    } else {
        format!("\n{}", resolved.cargo_profile_sections.join("\n"))
    };

    let content = format!(
        "{header}\
         # Source: profiles/{name}.profile\n\
         \n\
         [workspace]\n\
         members = [\n\
         {members}\n\
         ]\n\
         resolver = \"2\"\n\
         {pkg}{deps}{profiles}",
        header = GENERATED_HEADER,
        name = resolved.profile_name,
        members = member_lines,
        pkg = pkg_section,
        deps = deps_section,
        profiles = profiles_section,
    );

    fs::write(&out_path, &content).map_err(io_err(&out_path))?;

    Ok(out_path)
}

/// Create a new empty profile file at `profiles/<name>.profile`.
/// Initialises it with an empty `[implementations]` table.
pub fn create_profile(root: &Path, name: &str) -> Result<()> {
    let profiles_dir = root.join("profiles");
    fs::create_dir_all(&profiles_dir).map_err(io_err(&profiles_dir))?;
    let profile_path = profiles_dir.join(format!("{name}.profile"));
    if profile_path.exists() {
        return Err(ScaffoldError::Other(format!(
            "profile '{name}' already exists at {}",
            profile_path.display()
        )));
    }
    fs::write(&profile_path, "[implementations]\n").map_err(io_err(&profile_path))?;
    Ok(())
}

/// Add or update an implementation entry in a profile file.
///
/// Creates `profiles/<name>.profile` if it doesn't exist.
/// Adds or replaces the `[implementations]` entry for `interface`.
pub fn add_profile_impl(
    root: &Path,
    profile_name: &str,
    interface: &str,
    impl_path: &str,
) -> Result<()> {
    let profiles_dir = root.join("profiles");
    fs::create_dir_all(&profiles_dir).map_err(io_err(&profiles_dir))?;

    let profile_path = profiles_dir.join(format!("{}.profile", profile_name));

    let content = if profile_path.exists() {
        fs::read_to_string(&profile_path).map_err(io_err(&profile_path))?
    } else {
        String::new()
    };

    let mut doc: DocumentMut = content.parse().map_err(toml_err(&profile_path))?;

    // Ensure [implementations] table exists
    if doc.get("implementations").is_none() {
        doc["implementations"] = toml_edit::table();
    }
    doc["implementations"][interface] = toml_edit::value(impl_path);

    fs::write(&profile_path, doc.to_string()).map_err(io_err(&profile_path))?;

    Ok(())
}

/// Write or update an implementation entry directly to a profile file path.
///
/// Unlike `add_profile_impl`, this takes the absolute path to the `.profile`
/// file rather than deriving it from `root` + `profile_name`. Creates the file
/// with an empty `[implementations]` table if it doesn't exist.
pub fn write_profile_impl(profile_path: &Path, interface: &str, impl_path: &str) -> Result<()> {
    let content = if profile_path.exists() {
        fs::read_to_string(profile_path).map_err(io_err(profile_path))?
    } else {
        "[implementations]\n".to_string()
    };
    let mut doc: DocumentMut = content.parse().map_err(toml_err(profile_path))?;
    if doc.get("implementations").is_none() {
        doc["implementations"] = toml_edit::table();
    }
    doc["implementations"][interface] = toml_edit::value(impl_path);
    fs::write(profile_path, doc.to_string()).map_err(io_err(profile_path))?;
    Ok(())
}

/// Create `profiles/dev.profile` with an `[implementations]` section populated
/// from the given `(interface_key, path_string)` pairs.
/// Creates the `profiles/` directory if it doesn't exist.
pub fn create_dev_profile_from_deps(root: &Path, impls: &[(String, String)]) -> Result<()> {
    let profiles_dir = root.join("profiles");
    fs::create_dir_all(&profiles_dir).map_err(io_err(&profiles_dir))?;

    let profile_path = profiles_dir.join("dev.profile");

    let mut doc = toml_edit::DocumentMut::new();
    doc["implementations"] = toml_edit::table();
    for (key, path) in impls {
        doc["implementations"][key] = toml_edit::value(path.as_str());
    }

    fs::write(&profile_path, doc.to_string()).map_err(io_err(&profile_path))?;

    Ok(())
}


/// Write `Polylith.toml` from a pre-analysed `RootDemotionPlan`.
///
/// Writes only `Polylith.toml` — does NOT touch root `Cargo.toml`.
/// The caller is responsible for checking whether `Polylith.toml` already exists
/// before calling this function (and honouring the `--force` flag).
pub fn write_polylith_toml(root: &Path, plan: &RootDemotionPlan) -> Result<()> {
    let polylith_toml_path = root.join("Polylith.toml");

    // Build Polylith.toml content from the plan
    let mut polylith_content = String::new();
    polylith_content.push_str("[workspace]\n");
    polylith_content.push_str("schema_version = 1\n");

    if !plan.libraries.is_empty() {
        polylith_content.push_str("\n[libraries]\n");
        let mut sorted_keys: Vec<&String> = plan.libraries.keys().collect();
        sorted_keys.sort();
        for key in sorted_keys {
            let info = &plan.libraries[key];
            let rendered = info.raw.as_deref().unwrap_or("\"*\"");
            polylith_content.push_str(&format!("{} = {}\n", key, rendered));
        }
    }

    if !plan.profiles.is_empty() {
        polylith_content.push_str("\n[profiles]\n");
        let mut sorted_names: Vec<&String> = plan.profiles.keys().collect();
        sorted_names.sort();
        for name in sorted_names {
            let path = &plan.profiles[name];
            polylith_content.push_str(&format!("{} = \"{}\"\n", name, path));
        }
    }

    fs::write(&polylith_toml_path, &polylith_content).map_err(io_err(&polylith_toml_path))?;

    Ok(())
}

/// Strip `{ workspace = true }` references from all brick `Cargo.toml` files
/// under `components/`, `bases/`, and `projects/`, replacing them with explicit
/// values from `polylith_toml`. Only external library deps (those listed in
/// `polylith_toml.libraries`) are rewritten; inter-brick workspace deps are left
/// unchanged so that profiles can swap implementations.
/// Returns the number of bricks rewritten.
pub fn strip_workspace_inheritance(
    root: &Path,
    polylith_toml: &crate::workspace::PolylithToml,
    workspace_package: Option<&crate::workspace::WorkspacePackageMeta>,
) -> Result<usize> {
    let mut count = 0;
    for kind_dir in &["components", "bases", "projects"] {
        let dir = root.join(kind_dir);
        if !dir.exists() {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("Cargo.toml");
            if !manifest_path.exists() {
                continue;
            }
            let changed = strip_workspace_from_manifest(&manifest_path, polylith_toml, workspace_package)?;
            if changed {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Rewrite a single brick `Cargo.toml`, replacing `{ workspace = true }` fields
/// with explicit values from `polylith_toml`. Only external library deps (those in
/// `polylith_toml.libraries`) are rewritten; inter-brick workspace deps are left
/// unchanged so that profiles can swap implementations.
/// Returns `true` if the file was changed.
fn strip_workspace_from_manifest(
    manifest_path: &Path,
    polylith_toml: &crate::workspace::PolylithToml,
    workspace_package: Option<&crate::workspace::WorkspacePackageMeta>,
) -> Result<bool> {
    let content = fs::read_to_string(manifest_path).map_err(io_err(manifest_path))?;
    let mut doc: DocumentMut = content.parse().map_err(toml_err(manifest_path))?;

    let mut changed = false;

    // -- Package metadata fields --
    let pkg_meta = workspace_package;

    // We process fields: version, edition, authors, license, repository
    // We need to check if they are `{ workspace = true }` or dotted-key form.
    let pkg_fields: &[(&str, Option<String>)] = &[
        ("version", pkg_meta.and_then(|m| m.version.clone())),
        ("edition", pkg_meta.and_then(|m| m.edition.clone())),
        ("license", pkg_meta.and_then(|m| m.license.clone())),
        ("repository", pkg_meta.and_then(|m| m.repository.clone())),
    ];

    for (field, maybe_value) in pkg_fields {
        if is_workspace_true_item(doc.get("package").and_then(|p| p.get(field))) {
            if let Some(val) = maybe_value {
                doc["package"][field] = toml_edit::value(val.as_str());
                changed = true;
            }
        }
    }

    // Handle authors separately (it's an array)
    if is_workspace_true_item(doc.get("package").and_then(|p| p.get("authors"))) {
        if let Some(meta) = pkg_meta {
            if let Some(authors) = &meta.authors {
                let mut arr = toml_edit::Array::new();
                for author in authors {
                    arr.push(author.as_str());
                }
                doc["package"]["authors"] = toml_edit::value(arr);
                changed = true;
            }
        }
    }

    // -- Dependency tables --
    let dep_tables = ["dependencies", "dev-dependencies", "build-dependencies"];
    for table_name in &dep_tables {
        // Collect dep names that need to be rewritten (borrow ends before mutation)
        let dep_names: Vec<String> = doc
            .get(table_name)
            .and_then(|t| t.as_table())
            .map(|t| {
                t.iter()
                    .filter(|(_, v)| is_workspace_true_item(Some(v)))
                    .map(|(k, _)| k.to_string())
                    .collect()
            })
            .unwrap_or_default();

        for dep_name in dep_names {
            // Look up in polylith_toml.libraries
            if let Some(lib_info) = polylith_toml.libraries.get(&dep_name) {
                let new_val = if lib_info.features.is_empty() {
                    if let Some(ver) = &lib_info.version {
                        // Simple version string
                        toml_edit::Item::Value(toml_edit::Value::from(ver.as_str()))
                    } else if let Some(raw) = &lib_info.raw {
                        // Non-version dep (git, etc.) — parse the raw TOML value
                        match raw.parse::<toml_edit::Value>() {
                            Ok(v) => toml_edit::Item::Value(v),
                            Err(_) => {
                                eprintln!("warning: dep '{}' — could not parse raw value from Polylith.toml, left unchanged", dep_name);
                                continue;
                            }
                        }
                    } else {
                        eprintln!("warning: dep '{}' uses workspace = true but Polylith.toml has no version — left unchanged", dep_name);
                        continue;
                    }
                } else {
                    // Inline table with version and features
                    let mut tbl = toml_edit::InlineTable::new();
                    if let Some(ver) = &lib_info.version {
                        tbl.insert("version", toml_edit::Value::from(ver.as_str()));
                    }
                    let mut features_arr = toml_edit::Array::new();
                    for feat in &lib_info.features {
                        features_arr.push(feat.as_str());
                    }
                    tbl.insert("features", toml_edit::Value::Array(features_arr));
                    toml_edit::Item::Value(toml_edit::Value::InlineTable(tbl))
                };
                doc[table_name][&dep_name] = new_val;
                changed = true;
            } else {
                eprintln!(
                    "warning: dep '{}' uses workspace = true but is not in Polylith.toml [libraries] or [implementations] — left unchanged",
                    dep_name
                );
            }
        }
    }

    if changed {
        fs::write(manifest_path, doc.to_string()).map_err(io_err(manifest_path))?;
    }
    Ok(changed)
}

/// Migrate `[workspace.package]` metadata from `Polylith.toml` to root `Cargo.toml` `[package]`.
///
/// The caller is responsible for reading `[workspace.package]` via
/// `workspace::read_polylith_workspace_package` and passing the result here.
/// Passing a `WorkspacePackageMeta` signals that there is something to migrate.
///
/// - Overwrites existing values for fields declared in `ws_pkg` into root `Cargo.toml`
///   `[package]`. Fields not present in `ws_pkg` are left untouched.
/// - Removes `[workspace][package]` from `Polylith.toml`.
/// - Writes both files back.
/// - Returns a summary of migrated fields.
pub fn migrate_package_meta_to_cargo_toml(root: &Path, ws_pkg: crate::workspace::WorkspacePackageMeta) -> Result<String> {
    let polylith_toml_path = root.join("Polylith.toml");
    if !polylith_toml_path.exists() {
        return Err(ScaffoldError::Other("Polylith.toml not found".to_string()));
    }

    let poly_content = fs::read_to_string(&polylith_toml_path).map_err(io_err(&polylith_toml_path))?;
    let mut poly_doc: DocumentMut = poly_content.parse().map_err(toml_err(&polylith_toml_path))?;

    // Read root Cargo.toml
    let cargo_toml_path = root.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return Err(ScaffoldError::Other("root Cargo.toml not found".to_string()));
    }
    let cargo_content = fs::read_to_string(&cargo_toml_path).map_err(io_err(&cargo_toml_path))?;
    let mut cargo_doc: DocumentMut = cargo_content.parse().map_err(toml_err(&cargo_toml_path))?;

    if cargo_doc.get("package").is_none() {
        return Err(ScaffoldError::Other("root Cargo.toml has no [package] section".to_string()));
    }

    // Overwrite fields declared in [workspace.package]; untouched fields remain as-is
    let mut migrated = vec![];
    {
        let pkg = cargo_doc["package"].as_table_mut()
            .ok_or_else(|| ScaffoldError::Other("[package] is not a table".to_string()))?;
        if let Some(v) = &ws_pkg.version {
            pkg["version"] = toml_edit::value(v.as_str());
            migrated.push(format!("version = \"{}\"", v));
        }
        if let Some(e) = &ws_pkg.edition {
            pkg["edition"] = toml_edit::value(e.as_str());
            migrated.push(format!("edition = \"{}\"", e));
        }
        // `authors` is `Some` when the key was explicitly present in the source — even if empty.
        // This lets `authors = []` (explicit clear) pass through correctly.
        if let Some(authors) = &ws_pkg.authors {
            let mut arr = toml_edit::Array::new();
            for author in authors {
                arr.push(author.as_str());
            }
            pkg["authors"] = toml_edit::value(arr);
            migrated.push(format!("authors = [{}]", authors.iter().map(|a| format!("\"{}\"", a)).collect::<Vec<_>>().join(", ")));
        }
        if let Some(l) = &ws_pkg.license {
            pkg["license"] = toml_edit::value(l.as_str());
            migrated.push(format!("license = \"{}\"", l));
        }
        if let Some(r) = &ws_pkg.repository {
            pkg["repository"] = toml_edit::value(r.as_str());
            migrated.push(format!("repository = \"{}\"", r));
        }
    }

    // Remove [workspace.package] from Polylith.toml
    if let Some(ws) = poly_doc.get_mut("workspace").and_then(|w| w.as_table_mut()) {
        ws.remove("package");
    }

    // Write both files back
    fs::write(&cargo_toml_path, cargo_doc.to_string()).map_err(io_err(&cargo_toml_path))?;
    fs::write(&polylith_toml_path, poly_doc.to_string()).map_err(io_err(&polylith_toml_path))?;

    if migrated.is_empty() {
        Ok("Polylith.toml [workspace.package] removed; no recognized fields to migrate".to_string())
    } else {
        Ok(format!(
            "Migrated from Polylith.toml [workspace.package] to root Cargo.toml [package]: {}",
            migrated.join(", ")
        ))
    }
}

use crate::toml_utils::toml_bool;


/// Return `true` if the given `toml_edit::Item` is `{ workspace = true }` — either
/// as a dotted key table (`version.workspace = true`) or an inline table.
fn is_workspace_true_item(item: Option<&toml_edit::Item>) -> bool {
    item.is_some_and(|i| toml_bool(i, "workspace") == Some(true))
}

/// Append a member path to the root workspace `Cargo.toml` `[workspace].members` array
/// using `toml_edit` to preserve existing comments and formatting.
fn add_workspace_member(root: &Path, member: &str) -> Result<()> {
    let manifest_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path).map_err(io_err(&manifest_path))?;
    let mut doc: DocumentMut = content.parse().map_err(toml_err(&manifest_path))?;

    // In a pure Polylith workspace (Polylith.toml present, no [workspace] section in root
    // Cargo.toml), bricks are managed by profiles — not by the root workspace members list.
    // Skip silently to avoid creating a spurious [workspace] section.
    if root.join("Polylith.toml").exists() && doc.get("workspace").is_none() {
        return Ok(());
    }

    let workspace = doc
        .entry("workspace")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| ScaffoldError::Other("'workspace' is not a table".to_string()))?;

    let members = workspace
        .entry("members")
        .or_insert(toml_edit::array())
        .as_array_mut()
        .ok_or_else(|| ScaffoldError::Other("'workspace.members' is not an array".to_string()))?;

    // Avoid duplicates
    let already_present = members
        .iter()
        .any(|v| v.as_str() == Some(member));
    if !already_present {
        members.push(member);
    }

    fs::write(&manifest_path, doc.to_string()).map_err(io_err(&manifest_path))?;
    Ok(())
}

/// Write the new version to `Polylith.toml` `[versioning] version` field.
/// Uses `toml_edit` to preserve formatting and comments.
pub fn write_polylith_version(root: &Path, new_version: &str) -> Result<()> {
    let path = root.join("Polylith.toml");
    let content = fs::read_to_string(&path).map_err(io_err(&path))?;
    let mut doc: toml_edit::DocumentMut = content.parse().map_err(toml_err(&path))?;
    doc["versioning"]["version"] = toml_edit::value(new_version);
    fs::write(&path, doc.to_string()).map_err(io_err(&path))?;
    Ok(())
}

/// Write `[workspace.package] version` in a root Cargo.toml.
/// Creates `[workspace]` and `[workspace.package]` tables if they don't exist.
/// Uses `toml_edit` to preserve formatting and comments.
/// Update the version in `[workspace.package]` of the given `Cargo.toml`.
///
/// Returns `Ok(())` without writing anything if `[workspace.package]` does not
/// exist — the version source of truth is `Polylith.toml`, not `Cargo.toml`.
/// Tables are never created; only an existing `[workspace.package]` is updated.
pub fn write_workspace_package_version(cargo_toml_path: &Path, new_version: &str) -> Result<()> {
    let content = fs::read_to_string(cargo_toml_path).map_err(io_err(cargo_toml_path))?;
    let mut doc: toml_edit::DocumentMut = content.parse().map_err(toml_err(cargo_toml_path))?;

    // Only update if [workspace.package] already exists.
    let has_workspace_package = doc
        .get("workspace")
        .and_then(|ws| ws.get("package"))
        .is_some();

    if !has_workspace_package {
        return Ok(());
    }

    doc["workspace"]["package"]["version"] = toml_edit::value(new_version);
    fs::write(cargo_toml_path, doc.to_string()).map_err(io_err(cargo_toml_path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, content: &str) {
        fs::write(dir.path().join(name), content).unwrap();
    }

    fn read_file(dir: &TempDir, name: &str) -> String {
        fs::read_to_string(dir.path().join(name)).unwrap()
    }

    fn read_ws_pkg(dir: &TempDir) -> crate::workspace::WorkspacePackageMeta {
        crate::workspace::read_polylith_workspace_package(dir.path())
            .expect("read_polylith_workspace_package failed")
            .expect("expected Some(WorkspacePackageMeta), got None")
    }

    /// Bug fix: version in [workspace.package] overwrites placeholder in root [package]
    #[test]
    fn migrate_package_meta_overwrites_existing_version() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace.package]
version = "0.17.0"
"#);
        write_file(&dir, "Cargo.toml", r#"
[package]
name = "my-workspace"
version = "0.0.0"
"#);

        let ws_pkg = read_ws_pkg(&dir);
        let result = migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap();
        assert!(result.contains("version"), "expected version in result: {result}");

        let cargo = read_file(&dir, "Cargo.toml");
        let doc: toml_edit::DocumentMut = cargo.parse().unwrap();
        assert_eq!(
            doc["package"]["version"].as_str(),
            Some("0.17.0"),
            "version should be overwritten with value from Polylith.toml"
        );
    }

    /// Selective overwrite: only declared fields are changed; undeclared fields untouched
    #[test]
    fn migrate_package_meta_selective_overwrite() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace.package]
version = "1.2.3"
edition = "2021"
"#);
        write_file(&dir, "Cargo.toml", r#"
[package]
name = "foo"
version = "0.0.0"
edition = "2018"
description = "kept as-is"
"#);

        let ws_pkg = read_ws_pkg(&dir);
        migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap();

        let cargo = read_file(&dir, "Cargo.toml");
        let doc: toml_edit::DocumentMut = cargo.parse().unwrap();
        assert_eq!(doc["package"]["version"].as_str(), Some("1.2.3"), "version should be overwritten");
        assert_eq!(doc["package"]["edition"].as_str(), Some("2021"), "edition should be overwritten");
        assert_eq!(doc["package"]["name"].as_str(), Some("foo"), "name should be untouched");
        assert_eq!(doc["package"]["description"].as_str(), Some("kept as-is"), "description should be untouched");
    }

    /// [workspace.package] is removed from Polylith.toml after migration
    #[test]
    fn migrate_package_meta_removes_workspace_package_from_polylith_toml() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace]
name = "my-ws"

[workspace.package]
version = "1.0.0"
"#);
        write_file(&dir, "Cargo.toml", r#"
[package]
name = "my-ws"
version = "0.0.0"
"#);

        let ws_pkg = read_ws_pkg(&dir);
        migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap();

        let poly = read_file(&dir, "Polylith.toml");
        let doc: toml_edit::DocumentMut = poly.parse().unwrap();
        assert!(
            doc.get("workspace").and_then(|w| w.get("package")).is_none(),
            "Polylith.toml should no longer have [workspace.package]"
        );
    }

    /// No [workspace.package] means the reader returns Ok(None) — nothing to migrate
    ///
    /// This test now lives here as a reader-level test; the migrate function itself
    /// no longer handles the absent-section case (the caller is responsible).
    #[test]
    fn read_polylith_workspace_package_returns_none_when_section_absent() {
        let dir = TempDir::new().unwrap();
        let polylith_content = "[workspace]\nname = \"my-ws\"\n";
        let cargo_content = "[package]\nname = \"my-ws\"\nversion = \"1.0.0\"\n";
        write_file(&dir, "Polylith.toml", polylith_content);
        write_file(&dir, "Cargo.toml", cargo_content);

        let result = crate::workspace::read_polylith_workspace_package(dir.path())
            .expect("expected Ok, got Err");
        assert!(
            result.is_none(),
            "expected None when [workspace.package] is absent, got: {result:?}"
        );

        // Files should be unchanged
        assert_eq!(read_file(&dir, "Polylith.toml"), polylith_content);
        assert_eq!(read_file(&dir, "Cargo.toml"), cargo_content);
    }

    /// Missing root [package] section returns an error
    #[test]
    fn migrate_package_meta_missing_package_section_returns_error() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace.package]
version = "1.0.0"
"#);
        // Cargo.toml without a [package] section (workspace-only manifest)
        write_file(&dir, "Cargo.toml", r#"
[workspace]
members = []
"#);

        let ws_pkg = read_ws_pkg(&dir);
        let err = migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no [package] section"),
            "expected 'no [package] section' error, got: {msg}"
        );
    }

    /// authors array is overwritten when [workspace.package].authors is declared
    #[test]
    fn migrate_package_meta_overwrites_authors() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace.package]
authors = ["new1", "new2"]
"#);
        write_file(&dir, "Cargo.toml", r#"
[package]
name = "my-workspace"
version = "0.0.0"
authors = ["old"]
"#);

        let ws_pkg = read_ws_pkg(&dir);
        let result = migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap();
        assert!(result.contains("authors"), "expected authors in result: {result}");

        let cargo = read_file(&dir, "Cargo.toml");
        let doc: toml_edit::DocumentMut = cargo.parse().unwrap();
        let authors: Vec<&str> = doc["package"]["authors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(authors, vec!["new1", "new2"], "authors should be overwritten");
    }

    /// license and repository are overwritten when declared in [workspace.package]
    #[test]
    fn migrate_package_meta_overwrites_license_and_repository() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace.package]
license = "MIT"
repository = "https://new.example/repo"
"#);
        write_file(&dir, "Cargo.toml", r#"
[package]
name = "my-workspace"
version = "0.0.0"
license = "Apache-2.0"
repository = "https://old.example/repo"
"#);

        let ws_pkg = read_ws_pkg(&dir);
        migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap();

        let cargo = read_file(&dir, "Cargo.toml");
        let doc: toml_edit::DocumentMut = cargo.parse().unwrap();
        assert_eq!(doc["package"]["license"].as_str(), Some("MIT"), "license should be overwritten");
        assert_eq!(
            doc["package"]["repository"].as_str(),
            Some("https://new.example/repo"),
            "repository should be overwritten"
        );
    }

    /// Explicit `authors = []` in [workspace.package] clears root authors (presence is source of truth)
    ///
    /// With `authors: Option<Vec<String>>`, `Some(vec![])` means "key was present but empty" —
    /// the type system now carries the distinction that previously required a local `authors_present: bool`.
    #[test]
    fn migrate_package_meta_explicit_empty_authors_clears_root() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "Polylith.toml", r#"
[workspace.package]
authors = []
"#);
        write_file(&dir, "Cargo.toml", r#"
[package]
name = "my-workspace"
version = "0.0.0"
authors = ["someone"]
"#);

        let ws_pkg = read_ws_pkg(&dir);
        // Confirm the type system captures the presence of an empty array
        assert_eq!(ws_pkg.authors, Some(vec![]), "authors should be Some(empty), not None");

        let result = migrate_package_meta_to_cargo_toml(dir.path(), ws_pkg).unwrap();
        assert!(result.contains("authors"), "expected authors in result: {result}");

        let cargo = read_file(&dir, "Cargo.toml");
        let doc: toml_edit::DocumentMut = cargo.parse().unwrap();
        let authors: Vec<&str> = doc["package"]["authors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(authors.is_empty(), "root authors should be cleared to empty array, got: {authors:?}");
    }
}
