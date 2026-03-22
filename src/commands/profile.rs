use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::validate::validate_brick_name;
use crate::output::table;
use crate::workspace::{build_workspace_map, discover_profiles, resolve_profile_workspace, resolve_root};

pub fn list(json: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let profiles = discover_profiles(&root)?;
    if json {
        table::print_profiles_json(&profiles);
    } else {
        table::print_profiles(&profiles);
    }
    Ok(())
}

pub fn build(name: &str, no_build: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let profiles = discover_profiles(&root)?;
    let profile = profiles
        .into_iter()
        .find(|p| p.name == name)
        .with_context(|| format!("profile '{}' not found in profiles/", name))?;

    let resolved = resolve_profile_workspace(&root, &profile, &map);
    let generated = crate::scaffold::write_profile_workspace(&root, &resolved)?;
    println!("Generated {}", generated.display());

    if !no_build {
        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(&generated)
            .status()
            .context("failed to invoke cargo build")?;
        if !status.success() {
            anyhow::bail!("cargo build failed");
        }
    }

    Ok(())
}

pub fn add(
    interface: &str,
    impl_path: &str,
    profile_name: &str,
    workspace_root: Option<&Path>,
) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    crate::scaffold::add_profile_impl(&root, profile_name, interface, impl_path)?;
    println!("Updated profiles/{}.profile: {} → {}", profile_name, interface, impl_path);
    Ok(())
}

pub fn new(name: &str, workspace_root: Option<&Path>) -> Result<()> {
    validate_brick_name(name)?;
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    crate::scaffold::create_profile(&root, name)?;
    println!("Created profiles/{name}.profile");
    Ok(())
}
