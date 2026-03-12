use std::env;
use std::path::Path;

use anyhow::{bail, Result};

use crate::scaffold;
use crate::workspace::resolve_root;

pub fn new(name: &str, workspace_root: Option<&Path>) -> Result<()> {
    validate_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    scaffold::create_component(&root, name)?;
    println!("Created component '{name}' at components/{name}/");
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("component name cannot be empty");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("component name must contain only alphanumeric characters, underscores, or hyphens");
    }
    Ok(())
}
