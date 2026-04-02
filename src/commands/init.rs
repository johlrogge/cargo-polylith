use std::env;
use std::path::Path;

use anyhow::Result;

use crate::scaffold;
use crate::workspace::resolve_root;

pub fn run(workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let warnings = scaffold::init_workspace(&root)?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    println!("Initialised polylith workspace at {}", root.display());
    println!();
    println!("Next steps:");
    println!("  cargo polylith component new <name>  # create a component");
    println!("  cargo polylith base new <name>       # create a base (binary)");
    println!("  cargo polylith project new <name>    # create a project workspace");
    Ok(())
}
