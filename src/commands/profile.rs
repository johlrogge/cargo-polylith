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
    eprintln!("warning: `profile build` is deprecated — use `cargo polylith cargo --profile {} build` instead.", name);
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
    eprintln!("Generated {}", generated.display());

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

pub fn migrate(force: bool, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;

    // Check if already migrated: Polylith.toml existence is the canonical marker
    let polylith_toml_path = root.join("Polylith.toml");
    if polylith_toml_path.exists() {
        eprintln!("workspace already migrated — Polylith.toml already exists");
        return Ok(());
    }

    // Read root Cargo.toml to check current members using cargo_toml for reliability
    let manifest_path = root.join("Cargo.toml");
    let manifest = cargo_toml::Manifest::from_path(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    // Also check if members is already empty (legacy already-migrated state)
    let members_empty = manifest
        .workspace
        .as_ref()
        .map(|ws| ws.members.is_empty())
        .unwrap_or(true);

    if members_empty && !polylith_toml_path.exists() {
        eprintln!("workspace already migrated — root members is already empty");
        return Ok(());
    }

    // Check if profiles/dev.profile already exists
    let dev_profile_path = root.join("profiles/dev.profile");
    if dev_profile_path.exists() && !force {
        anyhow::bail!(
            "profiles/dev.profile already exists — use --force to overwrite"
        );
    }

    // Build workspace map to get interface deps
    let map = build_workspace_map(&root)?;

    // Collect interface deps: (key, path_string) pairs
    let mut impl_pairs: Vec<(String, String)> = map
        .root_workspace_interface_deps
        .iter()
        .map(|(key, dep)| (key.clone(), dep.path.clone()))
        .collect();
    impl_pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Create profiles/dev.profile with the impl entries
    crate::scaffold::create_dev_profile_from_deps(&root, &impl_pairs)?;
    eprintln!("Created profiles/dev.profile");

    // Demote root workspace: write Polylith.toml and remove [workspace] from Cargo.toml
    // Must happen before profile workspace generation so [workspace.package] is available
    crate::scaffold::demote_root_workspace(&root, force)?;
    eprintln!("Created Polylith.toml");
    eprintln!("Removed [workspace] from root Cargo.toml");

    // Re-read workspace map now that Polylith.toml exists — it will populate workspace_package
    let map = build_workspace_map(&root)?;

    // Discover the newly written profile and resolve + write the profile workspace
    let profiles = discover_profiles(&root)?;
    let dev_profile = profiles
        .into_iter()
        .find(|p| p.name == "dev")
        .context("dev profile not found after creation")?;
    let resolved = resolve_profile_workspace(&root, &dev_profile, &map);
    let generated = crate::scaffold::write_profile_workspace(&root, &resolved)?;
    eprintln!("Generated {}", generated.display());

    // Print summary
    println!();
    println!("Migration complete.");
    println!();
    if impl_pairs.is_empty() {
        println!("  No interface deps found in [workspace.dependencies].");
    } else {
        println!("  Migrated {} interface implementation(s) to profiles/dev.profile:", impl_pairs.len());
        for (key, path) in &impl_pairs {
            println!("    {} = \"{}\"", key, path);
        }
    }
    println!();
    println!("  Polylith.toml written — workspace metadata and library versions moved there.");
    println!("  [workspace] removed from root Cargo.toml.");
    println!();
    println!("New workflow:");
    println!("  cargo polylith cargo build       # build with dev profile (default)");
    println!("  cargo polylith cargo test        # test with dev profile");
    println!("  cargo polylith cargo --profile <name> build  # build with a named profile");

    Ok(())
}

pub fn run_cargo(profile_name: &str, cargo_args: &[String], workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let profiles = discover_profiles(&root)?;
    let profile = profiles
        .into_iter()
        .find(|p| p.name == profile_name)
        .with_context(|| {
            if profile_name == "dev" {
                "no dev profile found — run `cargo polylith profile migrate` to set one up".to_string()
            } else {
                format!("profile '{}' not found in profiles/", profile_name)
            }
        })?;

    let resolved = resolve_profile_workspace(&root, &profile, &map);
    let generated = crate::scaffold::write_profile_workspace(&root, &resolved)?;
    eprintln!("Generated {}", generated.display());

    // Place --manifest-path after the subcommand name so cargo sees it as a
    // per-subcommand option (not a global flag), and before the remaining args
    // so that `--` in user args is not misinterpreted.
    let (subcommand, rest) = cargo_args.split_first().map(|(s, r)| (s.as_str(), r)).unwrap_or(("", &[]));
    let status = std::process::Command::new("cargo")
        .arg(subcommand)
        .arg("--manifest-path")
        .arg(&generated)
        .args(rest)
        .status()
        .context("failed to invoke cargo")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
