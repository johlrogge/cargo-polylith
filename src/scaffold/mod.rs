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
pub fn create_component(root: &Path, name: &str) -> Result<()> {
    let dir = root.join("components").join(name);
    let src = dir.join("src");
    fs::create_dir_all(&src)
        .with_context(|| format!("creating {}", src.display()))?;

    fs::write(dir.join("Cargo.toml"), component_cargo_toml(name))
        .context("writing component Cargo.toml")?;
    fs::write(src.join("lib.rs"), component_lib_rs(name))
        .context("writing lib.rs")?;
    fs::write(src.join(format!("{name}.rs")), component_impl_rs())
        .context("writing impl file")?;

    add_workspace_member(root, &format!("components/{name}"))?;
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
