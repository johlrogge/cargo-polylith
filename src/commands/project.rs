use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::validate::validate_brick_name;
use crate::scaffold;
use crate::workspace::{build_workspace_map, resolve_root};

pub fn new(name: &str, workspace_root: Option<&Path>) -> Result<()> {
    validate_brick_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    scaffold::create_project(&root, name)?;
    println!("Created project '{name}' at projects/{name}/");
    println!("Edit projects/{name}/Cargo.toml to add base and component dependencies.");
    Ok(())
}

pub fn set_impl(
    project: &str,
    interface: &str,
    implementation: &str,
    workspace_root: Option<&Path>,
) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let proj = map
        .projects
        .iter()
        .find(|p| p.name == project)
        .with_context(|| format!("project '{project}' not found in workspace"))?;
    let comp = map
        .components
        .iter()
        .find(|c| c.name == implementation)
        .with_context(|| format!("component '{implementation}' not found in workspace"))?;
    scaffold::set_project_implementation(&proj.path, interface, &comp.path)?;
    println!("Set implementation of '{interface}' to '{implementation}' in project '{project}'");
    Ok(())
}
