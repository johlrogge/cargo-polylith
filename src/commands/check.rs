use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::CommandError;
use crate::output::table;
use crate::workspace::{
    build_workspace_map, check::is_warning_kind, check_profile, discover_profiles, resolve_root,
    run_checks,
};

pub fn run(json: bool, profile_name: Option<&str>, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    if !map.is_workspace {
        eprintln!(
            "warning: {} does not appear to be a polylith workspace (no [workspace] in Cargo.toml)",
            root.display()
        );
    }

    // Discover all profiles so orphan check can consider profile selections.
    let all_profiles = discover_profiles(&root).unwrap_or_default();

    let mut violations = run_checks(&map, &all_profiles);

    // If a profile name was given, also validate that profile.
    if let Some(name) = profile_name {
        let profile = all_profiles
            .into_iter()
            .find(|p| p.name == name)
            .with_context(|| format!("profile '{}' not found in profiles/", name))?;
        violations.extend(check_profile(&profile, &map));
    }

    if json {
        table::print_check_json(&violations);
    } else {
        table::print_check(&violations);
    }

    if violations.iter().any(|v| !is_warning_kind(&v.kind)) {
        anyhow::bail!(CommandError::ProcessExit(1));
    }

    Ok(())
}
