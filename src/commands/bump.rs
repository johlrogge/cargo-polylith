use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::scaffold;
use crate::workspace::{
    build_workspace_map, compute_bumped_version, model::VersioningPolicy, resolve_root, BumpLevel,
};

/// Run the bump command and return `(old_version, new_version)` on success.
pub fn run(level_str: &str, workspace_root: Option<&Path>) -> Result<(String, String)> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    // Validate: Polylith.toml exists with versioning configured.
    let polylith_toml = map
        .polylith_toml
        .as_ref()
        .context("Polylith.toml not found — run `cargo polylith init` first")?;

    // Validate: must be relaxed mode (strict not yet supported).
    match polylith_toml.versioning_policy {
        Some(VersioningPolicy::Relaxed) => {}
        Some(VersioningPolicy::Strict) => {
            anyhow::bail!("strict versioning mode is not yet supported by `bump` — only relaxed mode is implemented");
        }
        None => {
            anyhow::bail!("versioning not configured in Polylith.toml — add a [versioning] section with policy = \"relaxed\"");
        }
    }

    // Get current version.
    let current_version = polylith_toml
        .workspace_version
        .as_deref()
        .context("no workspace version set in Polylith.toml [versioning] section")?
        .to_owned();

    // Parse level, compute new version.
    let level: BumpLevel = level_str
        .parse()
        .map_err(|e: String| anyhow::anyhow!("{}", e))?;

    let new_version = compute_bumped_version(&current_version, level)?;
    let new_version_str = new_version.to_string();

    // Write to Polylith.toml.
    scaffold::write_polylith_version(&root, &new_version_str)
        .with_context(|| "failed to write new version to Polylith.toml")?;

    // Write to root Cargo.toml [workspace.package] version if [workspace.package] exists.
    let cargo_toml_path = root.join("Cargo.toml");
    if cargo_toml_path.exists() {
        scaffold::write_workspace_package_version(&cargo_toml_path, &new_version_str)
            .with_context(|| "failed to write new version to root Cargo.toml")?;
    }

    Ok((current_version, new_version_str))
}
