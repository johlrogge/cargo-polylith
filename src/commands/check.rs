use std::env;
use std::path::Path;

use anyhow::Result;

use crate::output::table;
use crate::workspace::{build_workspace_map, resolve_root, run_checks};

pub fn run(json: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let violations = run_checks(&map);

    if json {
        table::print_check_json(&violations);
    } else {
        table::print_check(&violations);
    }

    if violations
        .iter()
        .any(|v| v.kind != crate::workspace::ViolationKind::OrphanComponent)
    {
        // Orphans are warnings; everything else is an error exit.
        std::process::exit(1);
    }

    Ok(())
}
