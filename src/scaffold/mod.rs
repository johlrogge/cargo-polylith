pub mod templates;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

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

/// Create a new base under `<root>/bases/<name>/`.
pub fn create_base(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("bases").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src)
        .with_context(|| format!("creating {}", src.display()))?;

    fs::write(dir.join("Cargo.toml"), base_cargo_toml(name))
        .context("writing base Cargo.toml")?;
    fs::write(src.join("main.rs"), base_main_rs())
        .context("writing main.rs")?;

    add_workspace_member(root, &format!("bases/{name}"))?;
    Ok(())
}

/// Create a new project under `<root>/projects/<name>/`.
pub fn create_project(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("projects").join(name);
    fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;

    fs::write(dir.join("Cargo.toml"), project_cargo_toml(name))
        .context("writing project Cargo.toml")?;
    Ok(())
}

/// Set `[patch.crates-io].<interface> = { path = "<rel>" }` in a project's `Cargo.toml`,
/// creating the `[patch]` and `[patch.crates-io]` tables if they don't exist.
/// The path written is relative from `project_path/` to `component_path/`.
pub fn set_project_patch(
    project_path: &Path,
    interface: &str,
    component_path: &Path,
) -> Result<()> {
    let manifest_path = project_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing project Cargo.toml")?;

    let rel = relative_path(project_path, component_path);
    let rel_str = rel.to_string_lossy();

    // Ensure [patch] table
    if doc.get("patch").is_none() {
        doc["patch"] = toml_edit::table();
    }
    // Ensure [patch.crates-io] table
    if doc["patch"].get("crates-io").is_none() {
        doc["patch"]["crates-io"] = toml_edit::table();
    }

    // Set the inline table: interface = { path = "..." }
    let mut tbl = toml_edit::InlineTable::new();
    tbl.insert("path", toml_edit::Value::from(rel_str.as_ref()));
    doc["patch"]["crates-io"][interface] =
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
