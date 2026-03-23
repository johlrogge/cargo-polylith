pub mod templates;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::workspace::ResolvedProfileWorkspace;

use templates::*;

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

/// Declare a component implementation for an interface in a project's `[dependencies]`.
///
/// Writes:
/// ```toml
/// <interface> = { path = "<rel>" }                          # when pkg name == interface
/// <interface> = { path = "<rel>", package = "<pkg-name>" }  # when pkg name differs
/// ```
///
/// The `package` key is only included when the component's actual package name differs
/// from the interface alias — matching the pattern used in real-world polylith workspaces.
pub fn set_project_implementation(
    project_path: &Path,
    interface: &str,
    component_path: &Path,
) -> Result<()> {
    let manifest_path = project_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing project Cargo.toml")?;

    // Read the component's actual package name.
    let comp_manifest = component_path.join("Cargo.toml");
    let comp_content = fs::read_to_string(&comp_manifest)
        .with_context(|| format!("reading {}", comp_manifest.display()))?;
    let comp_doc: DocumentMut = comp_content.parse().context("parsing component Cargo.toml")?;
    let pkg_name = comp_doc["package"]["name"]
        .as_str()
        .unwrap_or(interface)
        .to_string();

    let rel = relative_path(project_path, component_path);
    let rel_str = rel.to_string_lossy();

    // Ensure [dependencies] table exists.
    if doc.get("dependencies").is_none() {
        doc["dependencies"] = toml_edit::table();
    }

    let mut tbl = toml_edit::InlineTable::new();
    tbl.insert("path", toml_edit::Value::from(rel_str.as_ref()));
    if pkg_name != interface {
        tbl.insert("package", toml_edit::Value::from(pkg_name.as_str()));
    }
    doc["dependencies"][interface] =
        toml_edit::Item::Value(toml_edit::Value::InlineTable(tbl));

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    Ok(())
}

/// Compute a relative path from `from_dir` to `to_dir` (both absolute).
/// Walks up with `..` components until a common ancestor is found, then appends
/// the remaining suffix of `to_dir`.
fn relative_path(from_dir: &Path, to_dir: &Path) -> std::path::PathBuf {
    use std::path::PathBuf;

    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = to_dir.components().collect();

    let common = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();

    let up = from.len() - common;
    let mut rel = PathBuf::new();
    for _ in 0..up {
        rel.push("..");
    }
    for part in &to[common..] {
        rel.push(part);
    }
    rel
}

/// Write a profile workspace Cargo.toml from pre-resolved profile data.
///
/// Creates `profiles/<name>/Cargo.toml` at the workspace root.
/// Returns the path to the generated file.
pub fn write_profile_workspace(
    root: &Path,
    resolved: &ResolvedProfileWorkspace,
) -> Result<std::path::PathBuf> {
    let profile_dir = root.join("profiles").join(&resolved.profile_name);
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("creating {}", profile_dir.display()))?;
    let out_path = profile_dir.join("Cargo.toml");

    let member_lines = resolved
        .members
        .iter()
        .map(|m| format!("    \"{}\"", m))
        .collect::<Vec<_>>()
        .join(",\n");

    let mut dep_lines = resolved.interface_dep_lines.clone();
    dep_lines.extend(resolved.library_dep_lines.iter().cloned());

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
         {deps}",
        name = resolved.profile_name,
        members = member_lines,
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

/// Read root `Cargo.toml` with `toml_edit`, set `[workspace].members` to an empty array,
/// and write back. Preserves all other content (including `[workspace.dependencies]`).
pub fn clear_root_members(root: &Path) -> Result<()> {
    let manifest_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing root Cargo.toml")?;

    // Clear by removing all entries from the existing array to preserve formatting,
    // falling back to replacing with a fresh empty array.
    if let Some(members) = doc["workspace"]["members"].as_array_mut() {
        // Clear all entries in place, preserving the array's position and formatting
        members.clear();
    } else {
        doc["workspace"]["members"] = toml_edit::array();
    }

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    Ok(())
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
