use std::env;
use std::path::Path;

use anyhow::Result;

use crate::output::table;
use crate::workspace::{build_workspace_map, check::is_warning_kind, resolve_root, run_checks};

pub fn run(json: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    if !map.is_workspace {
        eprintln!("warning: {} does not appear to be a polylith workspace (no [workspace] in Cargo.toml)", root.display());
    }

    let violations = run_checks(&map);

    if json {
        table::print_check_json(&violations);
    } else {
        table::print_check(&violations);
    }

    if violations.iter().any(|v| !is_warning_kind(&v.kind)) {
        // Warnings are exit 0; everything else is an error exit.
        std::process::exit(1);
    }

    Ok(())
}
