use std::env;

use anyhow::Result;

use crate::scaffold;
use crate::workspace::find_workspace_root;

pub fn new(name: &str) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = find_workspace_root(&cwd)?;
    scaffold::create_project(&root, name)?;
    println!("Created project '{name}' at projects/{name}/");
    println!("Edit projects/{name}/Cargo.toml to add bases as workspace members.");
    Ok(())
}
