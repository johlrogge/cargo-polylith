use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::commands::validate::validate_brick_name;
use crate::commands::CommandError;
use crate::output::table;
use crate::workspace::{build_workspace_map, collect_root_interface_deps, discover_profiles, plan_root_demotion, resolve_profile_workspace, resolve_root};

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

    // Check if profiles/dev.profile already exists
    let dev_profile_path = root.join("profiles/dev.profile");
    if dev_profile_path.exists() && !force {
        anyhow::bail!(
            "profiles/dev.profile already exists — use --force to overwrite"
        );
    }

    // Phase 1: collect only the interface path deps from root [workspace.dependencies].
    // We use the targeted helper rather than building the full WorkspaceMap because
    // Polylith.toml does not exist yet (demotion happens later in this function), and
    // we only need the interface wiring diagram to write profiles/dev.profile and to
    // strip workspace inheritance from bricks.  A full WorkspaceMap build would scan
    // all components/bases/projects unnecessarily at this stage.
    let root_interface_deps = collect_root_interface_deps(&root)?;

    // Collect interface deps: (key, path_string) pairs
    let mut impl_pairs: Vec<(String, String)> = root_interface_deps
        .iter()
        .map(|(key, dep)| (key.clone(), dep.path.clone()))
        .collect();
    impl_pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Create profiles/dev.profile with the impl entries
    crate::scaffold::create_dev_profile_from_deps(&root, &impl_pairs)?;
    eprintln!("Created profiles/dev.profile");

    // Write Polylith.toml: read/analyse phase (workspace module), then write phase (scaffold module).
    // Must happen before profile workspace generation so [workspace.package] is available.
    let demotion_plan = plan_root_demotion(&root)?;
    crate::scaffold::write_polylith_toml(&root, &demotion_plan)?;
    eprintln!("Created Polylith.toml");

    let polylith_toml = crate::workspace::read_polylith_toml(&root)?;
    let stripped_count = crate::scaffold::strip_workspace_inheritance(&root, &polylith_toml, demotion_plan.workspace_package.as_ref())?;
    if stripped_count > 0 {
        eprintln!("Stripped workspace inheritance from {} brick(s)", stripped_count);
    }

    // Phase 2: now that Polylith.toml has been written, build the full WorkspaceMap.
    // This second build is intentional and necessary: resolve_profile_workspace needs
    // workspace_package from PolylithToml (which only exists after write_polylith_toml
    // above), so we cannot reuse the Phase 1 result here.
    let map = build_workspace_map(&root)?;

    // Discover the newly written profile and resolve + regenerate root Cargo.toml from it
    let profiles = discover_profiles(&root)?;
    let dev_profile = profiles
        .into_iter()
        .find(|p| p.name == "dev")
        .context("dev profile not found after creation")?;
    let resolved = resolve_profile_workspace(&root, &dev_profile, &map);
    let generated = crate::scaffold::write_root_workspace_from_profile(&root, &resolved)?;
    eprintln!("Generated {}", generated.display());

    // Print summary
    println!();
    println!("Migration complete.");
    println!();
    if stripped_count > 0 {
        println!(
            "  Stripped workspace inheritance from {} brick(s) (explicit versions from Polylith.toml).",
            stripped_count
        );
        println!();
    }
    if impl_pairs.is_empty() {
        println!("  No interface deps found in [workspace.dependencies].");
    } else {
        println!("  Migrated {} interface implementation(s) to profiles/dev.profile:", impl_pairs.len());
        for (key, path) in &impl_pairs {
            println!("    {} = \"{}\"", key, path);
        }
    }
    println!();
    println!("  Polylith.toml written — library versions and workspace metadata stored there.");
    println!("  Root Cargo.toml regenerated from profiles/dev.profile.");
    println!();
    println!("New workflow:");
    println!("  cargo polylith cargo build                    # build with dev profile (default)");
    println!("  cargo polylith cargo test                     # test with dev profile");
    println!("  cargo polylith cargo --profile <name> build   # build with a named profile");
    println!();
    println!("  To switch active profile, use:");
    println!("    cargo polylith profile change-profile <name>");
    println!();
    println!("  Run `cargo build` or `cargo polylith cargo build` to build.");
    println!("  Note: bricks use `{{ workspace = true }}` deps which resolve via the root");
    println!("  workspace's [workspace.dependencies]. The root Cargo.toml is regenerated");
    println!("  from the active profile when running `cargo polylith cargo`.");

    Ok(())
}

pub fn change_profile(name: &str, workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let profiles = discover_profiles(&root)?;
    let profile = profiles
        .into_iter()
        .find(|p| p.name == name)
        .with_context(|| format!("profile '{}' not found in profiles/", name))?;

    let resolved = resolve_profile_workspace(&root, &profile, &map);
    let generated = crate::scaffold::write_root_workspace_from_profile(&root, &resolved)?;
    println!("Generated {}", generated.display());

    Ok(())
}

/// RAII guard that restores the root `Cargo.toml` to its original content
/// when dropped. This ensures cleanup happens even if cargo fails or panics.
struct CargoTomlGuard {
    path: PathBuf,
    backup: String,
}

impl Drop for CargoTomlGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::write(&self.path, &self.backup) {
            eprintln!("warning: failed to restore {}: {}", self.path.display(), e);
        }
    }
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

    // Back up the root Cargo.toml so we can restore it after the cargo run
    let cargo_toml_path = root.join("Cargo.toml");
    let backup = std::fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("failed to read {}", cargo_toml_path.display()))?;
    let _guard = CargoTomlGuard { path: cargo_toml_path, backup };

    let resolved = resolve_profile_workspace(&root, &profile, &map);
    let generated = crate::scaffold::write_root_workspace_from_profile(&root, &resolved)?;
    eprintln!("Generated {}", generated.display());

    if cargo_args.is_empty() {
        anyhow::bail!("no cargo subcommand specified");
    }
    let (subcommand, rest) = cargo_args.split_first().map(|(s, r)| (s.as_str(), r)).unwrap_or(("", &[]));
    let status = std::process::Command::new("cargo")
        .arg(subcommand)
        .args(rest)
        .status()
        .context("failed to invoke cargo")?;

    // _guard is dropped here (and also on early return/panic), restoring root Cargo.toml

    if !status.success() {
        anyhow::bail!(CommandError::ProcessExit(status.code().unwrap_or(1)));
    }

    Ok(())
}
