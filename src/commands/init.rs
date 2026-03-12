use std::env;

use anyhow::Result;

use crate::scaffold;

pub fn run() -> Result<()> {
    let root = env::current_dir()?;
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
