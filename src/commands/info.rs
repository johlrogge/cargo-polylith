use std::env;
use std::path::Path;

use anyhow::Result;

use crate::output::table;
use crate::workspace::{build_workspace_map, resolve_root};

pub fn run(json: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    if !map.is_workspace {
        eprintln!("warning: {} does not appear to be a polylith workspace (no [workspace] in Cargo.toml)", root.display());
    }

    if json {
        table::print_info_json(&map);
    } else {
        table::print_info(&map);
    }
    Ok(())
}
