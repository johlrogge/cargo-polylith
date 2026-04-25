use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::scaffold;
use crate::workspace::{
    build_workspace_map, compute_bumped_version, model::VersioningPolicy, resolve_root,
    root_cargo_toml_has_workspace_package,
    strict_bump::{analyze_brick_changes, compute_project_recommendations, ProjectBumpRecommendation},
    BumpLevel,
};
use crate::workspace::git;

/// Git's well-known empty tree hash. Used as a sentinel when no release tag exists,
/// causing all files to appear as "new" (not found at this ref).
const GIT_EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Result returned from the bump command.
pub enum BumpResult {
    /// Relaxed mode: workspace version was bumped.
    Relaxed { old: String, new: String },
    /// Strict mode: per-project recommendations.
    Strict {
        recommendations: Vec<ProjectBumpRecommendation>,
        /// Set when no prior release tag was found.
        no_prior_tag: bool,
    },
}

/// Run the bump command.
///
/// - If policy is `relaxed`: `level_str` is required.
/// - If policy is `strict`: analysis is automatic; `level_str` is ignored.
/// - `dry_run`: ignored for strict mode (strict is always analysis-only); relaxed always writes.
/// - `allow_dirty`: when false, refuse to bump if target files have uncommitted changes.
pub fn run(level_str: Option<&str>, workspace_root: Option<&Path>, dry_run: bool, allow_dirty: bool) -> Result<BumpResult> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    // Validate: Polylith.toml exists with versioning configured.
    let polylith_toml = map
        .polylith_toml
        .as_ref()
        .context("Polylith.toml not found — run `cargo polylith init` first")?;

    match polylith_toml.versioning_policy {
        Some(VersioningPolicy::Relaxed) => {
            run_relaxed(level_str, &root, &map, allow_dirty)
        }
        Some(VersioningPolicy::Strict) => {
            run_strict(&root, &map, dry_run, allow_dirty)
        }
        None => {
            anyhow::bail!("versioning not configured in Polylith.toml — add a [versioning] section with policy = \"relaxed\"");
        }
    }
}


fn run_relaxed(
    level_str: Option<&str>,
    root: &Path,
    map: &crate::workspace::WorkspaceMap,
    allow_dirty: bool,
) -> Result<BumpResult> {
    let level_str = level_str.context(
        "bump level required in relaxed mode — run `cargo polylith bump <major|minor|patch>`",
    )?;

    // Get current version.
    let polylith_toml = map.polylith_toml.as_ref().unwrap();
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

    // Refuse if target files have uncommitted modifications (unless --allow-dirty).
    if !allow_dirty {
        let mut dirty: Vec<&'static str> = Vec::new();
        if git::is_path_dirty(root, "Polylith.toml")? {
            dirty.push("Polylith.toml");
        }
        if root_cargo_toml_has_workspace_package(root)?
            && git::is_path_dirty(root, "Cargo.toml")?
        {
            dirty.push("Cargo.toml");
        }
        if !dirty.is_empty() {
            anyhow::bail!(
                "refusing to bump: uncommitted changes in {} — commit or stash, or pass --allow-dirty",
                dirty.join(", ")
            );
        }
    }

    // Write to Polylith.toml.
    scaffold::write_polylith_version(root, &new_version_str)
        .with_context(|| "failed to write new version to Polylith.toml")?;

    // Write to root Cargo.toml [workspace.package] version if [workspace.package] exists.
    let cargo_toml_path = root.join("Cargo.toml");
    if cargo_toml_path.exists() {
        scaffold::write_workspace_package_version(&cargo_toml_path, &new_version_str)
            .with_context(|| "failed to write new version to root Cargo.toml")?;
    }

    Ok(BumpResult::Relaxed { old: current_version, new: new_version_str })
}

fn run_strict(
    root: &Path,
    map: &crate::workspace::WorkspaceMap,
    _dry_run: bool,
    _allow_dirty: bool,
) -> Result<BumpResult> {
    let polylith_toml = map.polylith_toml.as_ref().unwrap();
    let tag_prefix = polylith_toml.tag_prefix.as_deref().unwrap_or("v");

    let tag_result = git::find_last_release_tag(root, tag_prefix)?;
    let no_prior_tag = tag_result.is_none();

    if no_prior_tag {
        eprintln!("note: no previous release tag found; all bricks treated as new");
    }

    let tag = tag_result.unwrap_or_else(|| GIT_EMPTY_TREE.to_string());

    let brick_changes = analyze_brick_changes(root, map, &tag)?;
    let recommendations = compute_project_recommendations(map, &brick_changes);

    // Strict mode is always analysis-only — apply version changes manually or use relaxed mode.
    eprintln!("note: strict mode provides analysis only — apply version changes manually or use relaxed mode for automatic bumps");

    Ok(BumpResult::Strict { recommendations, no_prior_tag })
}
