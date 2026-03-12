use std::env;
use std::path::Path;

use anyhow::Result;

use crate::output::table;
use crate::workspace::{build_workspace_map, resolve_root};

pub fn run(component: Option<&str>, json: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    if json {
        table::print_deps_json(&map, component);
    } else {
        table::print_deps(&map, component);
    }
    Ok(())
}
