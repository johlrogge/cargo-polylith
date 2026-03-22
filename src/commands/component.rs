use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::validate::validate_brick_name;
use crate::scaffold;
use crate::workspace::{build_workspace_map, resolve_root};

pub fn new(name: &str, interface: Option<&str>, workspace_root: Option<&Path>) -> Result<()> {
    validate_brick_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let iface = interface.unwrap_or(name);
    scaffold::create_component(&root, name, iface)?;
    println!("Created component '{name}' (interface: '{iface}') at components/{name}/");
    Ok(())
}

pub fn update(name: &str, interface: Option<&str>, workspace_root: Option<&Path>) -> Result<()> {
    validate_brick_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let comp = map
        .components
        .iter()
        .find(|c| c.name == name)
        .with_context(|| format!("component '{name}' not found in workspace"))?;
    let iface = interface.unwrap_or(name);
    scaffold::write_interface_to_toml(&comp.path, iface)?;
    println!("Updated component '{name}': interface = \"{iface}\"");
    Ok(())
}

