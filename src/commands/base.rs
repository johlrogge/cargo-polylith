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
    scaffold::create_base(&root, name)?;
    println!("Created base '{name}' at bases/{name}/");
    Ok(())
}

pub fn update(name: &str, test_base: bool, workspace_root: Option<&Path>) -> Result<()> {
    validate_brick_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let base = map
        .bases
        .iter()
        .find(|b| b.name == name)
        .with_context(|| format!("base '{name}' not found in workspace"))?;
    scaffold::write_test_base_to_toml(&base.path, test_base)?;
    println!("Updated base '{name}': test-base = {test_base}");
    Ok(())
}
