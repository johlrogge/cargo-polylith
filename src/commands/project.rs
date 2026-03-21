use std::env;
use std::path::Path;

use anyhow::Result;

use crate::scaffold;
use crate::workspace::resolve_root;

pub fn new(name: &str, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    scaffold::create_project(&root, name)?;
    println!("Created project '{name}' at projects/{name}/");
    println!("Edit projects/{name}/Cargo.toml to add base and component dependencies.");
    Ok(())
}
