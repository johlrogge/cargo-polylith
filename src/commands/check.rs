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

    let warning_kinds = [
        crate::workspace::ViolationKind::OrphanComponent,
        crate::workspace::ViolationKind::WildcardReExport,
        crate::workspace::ViolationKind::BaseHasMainRs,
        crate::workspace::ViolationKind::ProjectMissingBase,
        crate::workspace::ViolationKind::NotInRootWorkspace,
        crate::workspace::ViolationKind::AmbiguousInterface,
        crate::workspace::ViolationKind::DuplicateName,
        crate::workspace::ViolationKind::MissingInterface,
    ];
    if violations.iter().any(|v| !warning_kinds.contains(&v.kind)) {
        // Warnings are exit 0; everything else is an error exit.
        std::process::exit(1);
    }

    Ok(())
}
