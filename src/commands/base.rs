use std::env;
use std::path::Path;

use anyhow::{bail, Result};

use crate::scaffold;
use crate::workspace::resolve_root;

pub fn new(name: &str, workspace_root: Option<&Path>) -> Result<()> {
    validate_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    scaffold::create_base(&root, name)?;
    println!("Created base '{name}' at bases/{name}/");
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("base name cannot be empty");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("base name must contain only alphanumeric characters, underscores, or hyphens");
    }
    Ok(())
}
