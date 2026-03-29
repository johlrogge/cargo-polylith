pub mod templates;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::workspace::{ResolvedProfileWorkspace, RootDemotionPlan};

use templates::*;

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
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing Cargo.toml")?;

    if doc.get("dependencies").is_none() {
        doc["dependencies"] = toml_edit::table();
    }
    let deps = doc["dependencies"]
        .as_table_mut()
        .context("[dependencies] is not a table")?;

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

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
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
            fs::create_dir_all(&p)
                .with_context(|| format!("creating {}", p.display()))?;
        }
    }
    let cargo_dir = root.join(".cargo");
    fs::create_dir_all(&cargo_dir)
        .with_context(|| "creating .cargo directory")?;
    let config_path = cargo_dir.join("config.toml");
    if !config_path.exists() {
        fs::write(&config_path, cargo_config_toml())
            .with_context(|| format!("writing {}", config_path.display()))?;
    }
    Ok(warnings)
}

/// Create a new component under `<root>/components/<name>/`.
pub fn create_component(root: &Path, name: &str, interface: &str) -> Result<()> {
    let dir = root.join("components").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src)
        .with_context(|| format!("creating {}", src.display()))?;

    fs::write(dir.join("Cargo.toml"), component_cargo_toml(name, interface))
        .context("writing component Cargo.toml")?;
    fs::write(src.join("lib.rs"), component_lib_rs(name))
        .context("writing lib.rs")?;
    fs::write(src.join(format!("{name}.rs")), component_impl_rs())
        .context("writing impl file")?;

    add_workspace_member(root, &format!("components/{name}"))?;
    Ok(())
}

/// Write or update the `[package.metadata.polylith] interface` key in a component's
/// `Cargo.toml`. Creates the metadata tables if they don't exist.
pub fn write_interface_to_toml(component_path: &Path, interface: &str) -> Result<()> {
    let manifest_path = component_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing Cargo.toml")?;
    if doc["package"].get("metadata").is_none() {
        doc["package"]["metadata"] = toml_edit::table();
    }
    if doc["package"]["metadata"].get("polylith").is_none() {
        doc["package"]["metadata"]["polylith"] = toml_edit::table();
    }
    doc["package"]["metadata"]["polylith"]["interface"] = toml_edit::value(interface);
    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    Ok(())
}

/// Write or update the `[package.metadata.polylith] test-base` key in a base's `Cargo.toml`.
/// Creates the metadata tables if they don't exist.
pub fn write_test_base_to_toml(base_path: &Path, test_base: bool) -> Result<()> {
    let manifest_path = base_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing Cargo.toml")?;
    if doc["package"].get("metadata").is_none() {
        doc["package"]["metadata"] = toml_edit::table();
    }
    if doc["package"]["metadata"].get("polylith").is_none() {
        doc["package"]["metadata"]["polylith"] = toml_edit::table();
    }
    doc["package"]["metadata"]["polylith"]["test-base"] = toml_edit::value(test_base);
    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    Ok(())
}

/// Create a new base under `<root>/bases/<name>/`.
pub fn create_base(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("bases").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src)
        .with_context(|| format!("creating {}", src.display()))?;

    fs::write(dir.join("Cargo.toml"), base_cargo_toml(name))
        .context("writing base Cargo.toml")?;
    fs::write(src.join("lib.rs"), base_lib_rs())
        .context("writing lib.rs")?;
    fs::write(src.join("main.rs"), base_main_rs())
        .context("writing main.rs")?;

    add_workspace_member(root, &format!("bases/{name}"))?;
    Ok(())
}

/// Create a new project under `<root>/projects/<name>/`.
pub fn create_project(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("projects").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src)
        .with_context(|| format!("creating {}", src.display()))?;

    fs::write(dir.join("Cargo.toml"), project_cargo_toml(name))
        .context("writing project Cargo.toml")?;
    fs::write(src.join("main.rs"), "fn main() {}\n")
        .context("writing project src/main.rs")?;

    add_workspace_member(root, &format!("projects/{name}"))?;
    Ok(())
}

/// Write a profile workspace Cargo.toml from pre-resolved profile data.
///
/// Creates `profiles/<name>/Cargo.toml` at the workspace root, plus symlinks
/// `profiles/<name>/components`, `profiles/<name>/bases`, and
/// `profiles/<name>/projects` pointing to the real source directories at the
/// workspace root (only when those directories exist).
///
/// Returns the path to the generated file.
pub fn write_profile_workspace(
    root: &Path,
    resolved: &ResolvedProfileWorkspace,
) -> Result<std::path::PathBuf> {
    let profile_dir = root.join("profiles").join(&resolved.profile_name);
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("creating {}", profile_dir.display()))?;

    // Create symlinks for each top-level brick directory that exists at root.
    // The symlink target is relative (../../<dir>) so it works regardless of
    // where the workspace is checked out.
    for dir_name in &["components", "bases", "projects"] {
        let src = root.join(dir_name);
        if src.exists() {
            let link = profile_dir.join(dir_name);
            if link.exists() || link.is_symlink() {
                // Already present — skip (idempotent).
            } else {
                #[cfg(unix)]
                std::os::unix::fs::symlink(
                    format!("../../{dir_name}"),
                    &link,
                )
                .with_context(|| format!("creating symlink {}", link.display()))?;
                #[cfg(not(unix))]
                {
                    // On non-Unix platforms symlinks require elevated privileges;
                    // skip silently and let the user create them manually if needed.
                    let _ = link;
                }
            }
        }
    }

    let out_path = profile_dir.join("Cargo.toml");

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
        if !pkg.authors.is_empty() {
            let authors_list = pkg
                .authors
                .iter()
                .map(|a| format!("\"{}\"", a))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("authors = [{}]", authors_list));
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

    let content = format!(
        "# Generated by cargo polylith — do not edit manually.\n\
         # Source: profiles/{name}.profile\n\
         \n\
         [workspace]\n\
         members = [\n\
         {members}\n\
         ]\n\
         resolver = \"2\"\n\
         {pkg}{deps}",
        name = resolved.profile_name,
        members = member_lines,
        pkg = pkg_section,
        deps = deps_section,
    );

    fs::write(&out_path, &content)
        .with_context(|| format!("writing {}", out_path.display()))?;

    Ok(out_path)
}

/// Create a new empty profile file at `profiles/<name>.profile`.
/// Initialises it with an empty `[implementations]` table.
pub fn create_profile(root: &Path, name: &str) -> Result<()> {
    let profiles_dir = root.join("profiles");
    fs::create_dir_all(&profiles_dir)
        .with_context(|| format!("creating {}", profiles_dir.display()))?;
    let profile_path = profiles_dir.join(format!("{name}.profile"));
    if profile_path.exists() {
        anyhow::bail!("profile '{name}' already exists at {}", profile_path.display());
    }
    fs::write(&profile_path, "[implementations]\n")
        .with_context(|| format!("writing {}", profile_path.display()))?;
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
    fs::create_dir_all(&profiles_dir)
        .with_context(|| format!("creating {}", profiles_dir.display()))?;

    let profile_path = profiles_dir.join(format!("{}.profile", profile_name));

    let content = if profile_path.exists() {
        fs::read_to_string(&profile_path)
            .with_context(|| format!("reading {}", profile_path.display()))?
    } else {
        String::new()
    };

    let mut doc: DocumentMut = content.parse()
        .with_context(|| format!("parsing {}", profile_path.display()))?;

    // Ensure [implementations] table exists
    if doc.get("implementations").is_none() {
        doc["implementations"] = toml_edit::table();
    }
    doc["implementations"][interface] = toml_edit::value(impl_path);

    fs::write(&profile_path, doc.to_string())
        .with_context(|| format!("writing {}", profile_path.display()))?;

    Ok(())
}

/// Write or update an implementation entry directly to a profile file path.
///
/// Unlike `add_profile_impl`, this takes the absolute path to the `.profile`
/// file rather than deriving it from `root` + `profile_name`. Creates the file
/// with an empty `[implementations]` table if it doesn't exist.
pub fn write_profile_impl(profile_path: &Path, interface: &str, impl_path: &str) -> Result<()> {
    let content = if profile_path.exists() {
        fs::read_to_string(profile_path)
            .with_context(|| format!("reading {}", profile_path.display()))?
    } else {
        "[implementations]\n".to_string()
    };
    let mut doc: DocumentMut = content.parse().context("parsing profile file")?;
    if doc.get("implementations").is_none() {
        doc["implementations"] = toml_edit::table();
    }
    doc["implementations"][interface] = toml_edit::value(impl_path);
    fs::write(profile_path, doc.to_string())
        .with_context(|| format!("writing {}", profile_path.display()))?;
    Ok(())
}

/// Create `profiles/dev.profile` with an `[implementations]` section populated
/// from the given `(interface_key, path_string)` pairs.
/// Creates the `profiles/` directory if it doesn't exist.
pub fn create_dev_profile_from_deps(root: &Path, impls: &[(String, String)]) -> Result<()> {
    let profiles_dir = root.join("profiles");
    fs::create_dir_all(&profiles_dir)
        .with_context(|| format!("creating {}", profiles_dir.display()))?;

    let profile_path = profiles_dir.join("dev.profile");

    let mut doc = toml_edit::DocumentMut::new();
    doc["implementations"] = toml_edit::table();
    for (key, path) in impls {
        doc["implementations"][key] = toml_edit::value(path.as_str());
    }

    fs::write(&profile_path, doc.to_string())
        .with_context(|| format!("writing {}", profile_path.display()))?;

    Ok(())
}


/// Execute root workspace demotion using a pre-analysed `RootDemotionPlan` (write-only phase).
///
/// 1. Writes `Polylith.toml` from plan data
/// 2. Strips `[workspace]` from root `Cargo.toml`
/// 3. Adds a `[package]` placeholder and creates `src/lib.rs` if there was no `[package]`
///
/// The caller is responsible for checking whether `Polylith.toml` already exists before
/// calling this function (and honouring the `--force` flag).
pub fn execute_root_demotion(root: &Path, plan: &RootDemotionPlan) -> Result<()> {
    let polylith_toml_path = root.join("Polylith.toml");

    // Build Polylith.toml content from the plan
    let mut polylith_content = String::new();
    polylith_content.push_str("[workspace]\n");
    polylith_content.push_str("schema_version = 1\n");

    if let Some(pkg) = &plan.workspace_package {
        polylith_content.push_str("\n[workspace.package]\n");
        if let Some(v) = &pkg.version {
            polylith_content.push_str(&format!("version = \"{}\"\n", v));
        }
        if let Some(e) = &pkg.edition {
            polylith_content.push_str(&format!("edition = \"{}\"\n", e));
        }
        if !pkg.authors.is_empty() {
            let authors_list = pkg.authors.iter().map(|a| format!("\"{}\"", a)).collect::<Vec<_>>().join(", ");
            polylith_content.push_str(&format!("authors = [{}]\n", authors_list));
        }
        if let Some(l) = &pkg.license {
            polylith_content.push_str(&format!("license = \"{}\"\n", l));
        }
        if let Some(r) = &pkg.repository {
            polylith_content.push_str(&format!("repository = \"{}\"\n", r));
        }
    }

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

    fs::write(&polylith_toml_path, &polylith_content)
        .with_context(|| format!("writing {}", polylith_toml_path.display()))?;

    // Remove [workspace] from root Cargo.toml entirely (read then write back)
    let manifest_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing root Cargo.toml")?;

    doc.remove("workspace");

    // Ensure the root Cargo.toml has a [package] section so Cargo can parse it.
    // Without [package] or [workspace], Cargo errors when walking up from bricks.
    // A dummy unpublished package is fine — Cargo walks past it (no [workspace])
    // so profile workspaces can still claim the bricks as members.
    if doc.get("package").is_none() {
        let mut pkg = toml_edit::table();
        pkg["name"] = toml_edit::value("workspace-root");
        pkg["version"] = toml_edit::value("0.0.0");
        pkg["edition"] = toml_edit::value("2021");
        pkg["publish"] = toml_edit::value(false);
        doc.insert("package", pkg);
        // Create an empty src/lib.rs so Cargo finds a valid target for this package.
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).with_context(|| format!("creating {}", src_dir.display()))?;
        let lib_rs = src_dir.join("lib.rs");
        if !lib_rs.exists() {
            fs::write(&lib_rs, "// Polylith workspace root placeholder — do not edit.\n")
                .with_context(|| format!("writing {}", lib_rs.display()))?;
        }
    }

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;

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
    _interface_impls: &[(String, String)],
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
            let changed = strip_workspace_from_manifest(&manifest_path, polylith_toml)?;
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
) -> Result<bool> {
    let content = fs::read_to_string(manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse()
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    let mut changed = false;

    // -- Package metadata fields --
    let pkg_meta = polylith_toml.workspace_package.as_ref();

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
            if !meta.authors.is_empty() {
                let mut arr = toml_edit::Array::new();
                for author in &meta.authors {
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
        fs::write(manifest_path, doc.to_string())
            .with_context(|| format!("writing {}", manifest_path.display()))?;
    }
    Ok(changed)
}

/// Extract a bool value from a TOML item by key (inline or regular table).
fn toml_bool(item: &toml_edit::Item, key: &str) -> Option<bool> {
    item.as_value()
        .and_then(|v| v.as_inline_table())
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_bool())
        .or_else(|| {
            item.as_table()
                .and_then(|t| t.get(key))
                .and_then(|i| i.as_value())
                .and_then(|v| v.as_bool())
        })
}


/// Return `true` if the given `toml_edit::Item` is `{ workspace = true }` — either
/// as a dotted key table (`version.workspace = true`) or an inline table.
fn is_workspace_true_item(item: Option<&toml_edit::Item>) -> bool {
    item.is_some_and(|i| toml_bool(i, "workspace") == Some(true))
}

/// Append a member path to the root workspace `Cargo.toml` `[workspace].members` array
/// using `toml_edit` to preserve existing comments and formatting.
fn add_workspace_member(root: &Path, member: &str) -> Result<()> {
    let manifest_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content
        .parse()
        .with_context(|| "parsing root Cargo.toml")?;

    let workspace = doc
        .entry("workspace")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .context("'workspace' is not a table")?;

    let members = workspace
        .entry("members")
        .or_insert(toml_edit::array())
        .as_array_mut()
        .context("'workspace.members' is not an array")?;

    // Avoid duplicates
    let already_present = members
        .iter()
        .any(|v| v.as_str() == Some(member));
    if !already_present {
        members.push(member);
    }

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    Ok(())
}
