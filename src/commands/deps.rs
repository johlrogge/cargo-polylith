use std::env;

use anyhow::Result;

use crate::output::table;
use crate::workspace::{build_workspace_map, find_workspace_root};

pub fn run(component: Option<&str>, json: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = find_workspace_root(&cwd)?;
    let map = build_workspace_map(&root)?;

    if json {
        // Minimal JSON output — full implementation in Phase 3
        println!("{{\"bases\": []}}");
        return Ok(());
    }

    table::print_deps(&map, component);
    Ok(())
}
