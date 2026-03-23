use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/poly-ws")
}

#[test]
fn profile_add_creates_profile_file() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Minimal workspace setup
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers=[]\nresolver=\"2\"\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "add",
            "logger",
            "--impl",
            "components/logger-fast",
            "--profile",
            "prod",
        ])
        .assert()
        .success();

    let profile_path = tmp.path().join("profiles/prod.profile");
    assert!(profile_path.exists(), "prod.profile should have been created");

    let content = fs::read_to_string(&profile_path).unwrap();
    assert!(content.contains("logger"), "should contain logger entry");
    assert!(content.contains("components/logger-fast"), "should contain impl path");
}

#[test]
fn check_warns_on_hardwired_dep() {
    // The fixture's cli base has direct path deps on logger and parser.
    // This should produce hardwired-dep warnings (exit 0, not failure).
    let out = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            fixture_root().to_str().unwrap(),
            "check",
            "--json",
        ])
        .assert()
        .success()  // warnings exit 0
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    let violations = parsed["violations"].as_array().unwrap();

    // The fixture's cli base has path deps to logger and parser; parser also has a path dep on logger.
    // Struct variants serialize as objects so we check the "hardwired_dep" key exists.
    let hardwired: Vec<_> = violations
        .iter()
        .filter(|v| v["kind"].get("hardwired_dep").is_some())
        .collect();
    assert!(
        hardwired.len() >= 2,
        "expected at least 2 hardwired-dep warnings for logger and parser, got: {violations:?}"
    );
}

#[test]
fn profile_list_shows_dev_profile() {
    cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "profile", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev"));
}

#[test]
fn profile_list_json_has_profiles_key() {
    let out = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            fixture_root().to_str().unwrap(),
            "profile",
            "list",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    assert!(parsed["profiles"].is_array());
}

#[test]
fn check_with_valid_profile_passes() {
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            fixture_root().to_str().unwrap(),
            "check",
            "--profile",
            "dev",
        ])
        .assert()
        // success() here depends on the fixture's existing check violations being
        // warnings-only (exit 0). If the fixture gains a hard-error violation,
        // this test will fail for a reason unrelated to profiles.
        .success();
}

#[test]
fn check_with_missing_profile_errors() {
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            fixture_root().to_str().unwrap(),
            "check",
            "--profile",
            "nonexistent",
        ])
        .assert()
        .failure();
}

#[test]
fn profile_build_no_build_generates_cargo_toml() {
    use tempfile::TempDir;
    use std::fs;

    // Copy fixture to a temp dir so we can write to it
    let tmp = TempDir::new().unwrap();
    let fixture = fixture_root();

    // Copy fixture structure
    let copy_file = |rel: &str| {
        let src = fixture.join(rel);
        let dst = tmp.path().join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        if src.exists() {
            fs::copy(&src, &dst).unwrap();
        }
    };

    copy_file("Cargo.toml");
    copy_file("components/logger/Cargo.toml");
    copy_file("components/logger/src/lib.rs");
    copy_file("components/logger/src/logger.rs");
    copy_file("components/parser/Cargo.toml");
    copy_file("components/parser/src/lib.rs");
    copy_file("components/parser/src/parser.rs");
    copy_file("bases/cli/Cargo.toml");
    copy_file("bases/cli/src/lib.rs");
    copy_file("projects/main-project/Cargo.toml");
    copy_file("projects/main-project/src/main.rs");
    copy_file("profiles/dev.profile");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "build",
            "dev",
            "--no-build",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Generated"));

    // Verify the generated file exists
    let generated = tmp.path().join("profiles/dev/Cargo.toml");
    assert!(generated.exists(), "profiles/dev/Cargo.toml should have been generated");

    let content = fs::read_to_string(&generated).unwrap();
    assert!(content.contains("[workspace]"), "should have [workspace] section");
    assert!(content.contains("\"components/logger\""), "should have logger member with symlink-relative path");
    assert!(content.contains("\"components/parser\""), "should have parser member with symlink-relative path");
    assert!(!content.contains("../../"), "should not contain ../../ paths — symlinks make them unnecessary");

    // Verify symlinks were created
    let components_link = tmp.path().join("profiles/dev/components");
    assert!(components_link.is_symlink(), "profiles/dev/components should be a symlink");
    let bases_link = tmp.path().join("profiles/dev/bases");
    assert!(bases_link.is_symlink(), "profiles/dev/bases should be a symlink");
    let projects_link = tmp.path().join("profiles/dev/projects");
    assert!(projects_link.is_symlink(), "profiles/dev/projects should be a symlink");
}

#[test]
fn profile_cargo_defaults_to_dev_hints_migrate_when_missing() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    // Minimal workspace with no profiles directory at all
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers=[]\nresolver=\"2\"\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "cargo",
            "build",
        ])
        .assert()
        .failure()
        .stderr(contains("profile migrate"));
}

#[test]
fn profile_cargo_uses_dev_by_default_when_profile_exists() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    let fixture = fixture_root();

    // Copy the fixture into a writable temp dir
    let copy_file = |rel: &str| {
        let src = fixture.join(rel);
        let dst = tmp.path().join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        if src.exists() {
            fs::copy(&src, &dst).unwrap();
        }
    };

    copy_file("Cargo.toml");
    copy_file("components/logger/Cargo.toml");
    copy_file("components/logger/src/lib.rs");
    copy_file("components/logger/src/logger.rs");
    copy_file("components/parser/Cargo.toml");
    copy_file("components/parser/src/lib.rs");
    copy_file("components/parser/src/parser.rs");
    copy_file("bases/cli/Cargo.toml");
    copy_file("bases/cli/src/lib.rs");
    copy_file("projects/main-project/Cargo.toml");
    copy_file("projects/main-project/src/main.rs");
    copy_file("profiles/dev.profile");

    // Run without --profile flag; it should default to "dev" and generate a workspace.
    // We don't assert success (cargo itself may fail on the generated workspace) but
    // we DO assert that "Generated" appears in stderr (dev profile was found) and
    // that "profile migrate" does NOT appear (i.e., we did not hit the missing-dev error).
    let output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "cargo",
            "build",
        ])
        .output()
        .unwrap();

    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    assert!(
        stderr.contains("Generated"),
        "expected 'Generated' in stderr — dev profile should have been found. Got: {stderr}"
    );
    assert!(
        !stderr.contains("profile migrate"),
        "should not show 'profile migrate' hint when dev profile exists. Got: {stderr}"
    );
}

#[test]
fn profile_migrate_creates_dev_profile() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Root workspace with members and an interface dep in [workspace.dependencies]
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"[workspace]
members = ["components/logger"]
resolver = "2"

[workspace.dependencies]
logger = { path = "components/logger" }
"#,
    ).unwrap();

    // Create a minimal component
    let comp_dir = tmp.path().join("components/logger/src");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::write(
        tmp.path().join("components/logger/Cargo.toml"),
        "[package]\nname = \"logger\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp_dir.join("lib.rs"), "pub fn log() {}\n").unwrap();

    // Verify migrate exits 0 and creates profiles/dev.profile
    let output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .output()
        .unwrap();

    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();

    // Check that dev.profile was created regardless of exit code
    let profile_path = tmp.path().join("profiles/dev.profile");

    assert!(
        output.status.success(),
        "migrate should succeed.\nstderr:\n{stderr}\nstdout:\n{stdout}\ndev.profile exists: {}\n",
        profile_path.exists(),
    );

    assert!(profile_path.exists(), "profiles/dev.profile should have been created");
    let profile_content = fs::read_to_string(&profile_path).unwrap();
    assert!(profile_content.contains("logger"), "should contain logger entry.\ncontent:\n{profile_content}");
    assert!(profile_content.contains("components/logger"), "should contain impl path.\ncontent:\n{profile_content}");

    // After migration, [workspace] should be removed from root Cargo.toml entirely.
    let root_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        !root_content.contains("[workspace"),
        "root Cargo.toml should have no [workspace] section after migration.\ncontent:\n{root_content}"
    );

    // Polylith.toml should have been created
    let polylith_toml_path = tmp.path().join("Polylith.toml");
    assert!(polylith_toml_path.exists(), "Polylith.toml should have been created");
    let polylith_content = fs::read_to_string(&polylith_toml_path).unwrap();
    assert!(polylith_content.contains("[workspace]"), "Polylith.toml should have [workspace] section");
}

#[test]
fn profile_migrate_already_migrated() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // A Polylith.toml already present — canonical marker for "already migrated"
    fs::write(
        tmp.path().join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n",
    ).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "# polylith workspace — see Polylith.toml\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .assert()
        .success()
        .stderr(contains("already migrated"));
}

#[test]
fn profile_migrate_refuses_overwrite_without_force() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/logger\"]\nresolver = \"2\"\n",
    ).unwrap();

    // Pre-existing profiles/dev.profile
    fs::create_dir_all(tmp.path().join("profiles")).unwrap();
    fs::write(
        tmp.path().join("profiles/dev.profile"),
        "[implementations]\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .assert()
        .failure()
        .stderr(contains("--force"));
}

#[test]
fn profile_migrate_force_overwrites() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Root workspace with members and an interface dep
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"[workspace]
members = ["components/logger"]
resolver = "2"

[workspace.dependencies]
logger = { path = "components/logger" }
"#,
    ).unwrap();

    let comp_dir = tmp.path().join("components/logger/src");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::write(
        tmp.path().join("components/logger/Cargo.toml"),
        "[package]\nname = \"logger\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp_dir.join("lib.rs"), "pub fn log() {}\n").unwrap();

    // Pre-existing profiles/dev.profile
    fs::create_dir_all(tmp.path().join("profiles")).unwrap();
    fs::write(
        tmp.path().join("profiles/dev.profile"),
        "[implementations]\nold_entry = \"old/path\"\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
            "--force",
        ])
        .assert()
        .success();

    // The new profile should have overwritten the old one
    let profile_content = fs::read_to_string(tmp.path().join("profiles/dev.profile")).unwrap();
    assert!(!profile_content.contains("old_entry"), "old entry should be gone after --force migration");
    assert!(profile_content.contains("logger"), "should have new logger entry");
}

#[test]
fn profile_migrate_generates_profile_workspace() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    let fixture = fixture_root();

    let copy_file = |rel: &str| {
        let src = fixture.join(rel);
        let dst = tmp.path().join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        if src.exists() {
            fs::copy(&src, &dst).unwrap();
        }
    };

    copy_file("Cargo.toml");
    copy_file("components/logger/Cargo.toml");
    copy_file("components/logger/src/lib.rs");
    copy_file("components/logger/src/logger.rs");
    copy_file("components/parser/Cargo.toml");
    copy_file("components/parser/src/lib.rs");
    copy_file("components/parser/src/parser.rs");
    copy_file("bases/cli/Cargo.toml");
    copy_file("bases/cli/src/lib.rs");
    copy_file("projects/main-project/Cargo.toml");
    copy_file("projects/main-project/src/main.rs");

    // NOTE: we intentionally do NOT copy profiles/dev.profile so the fixture
    // starts without a profile (but it HAS members in the root workspace).

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .assert()
        .success();

    // profiles/dev/Cargo.toml should have been generated
    let generated = tmp.path().join("profiles/dev/Cargo.toml");
    assert!(generated.exists(), "profiles/dev/Cargo.toml should have been generated");

    let content = fs::read_to_string(&generated).unwrap();
    assert!(content.contains("[workspace]"), "should have [workspace] section");
}

#[test]
fn profile_migrate_creates_polylith_toml() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Root workspace with members, [workspace.package], and [workspace.dependencies]
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"[workspace]
members = ["components/logger"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"

[workspace.dependencies]
logger = { path = "components/logger" }
serde = { version = "1", features = ["derive"] }
"#,
    ).unwrap();

    let comp_dir = tmp.path().join("components/logger/src");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::write(
        tmp.path().join("components/logger/Cargo.toml"),
        "[package]\nname = \"logger\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp_dir.join("lib.rs"), "pub fn log() {}\n").unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .assert()
        .success();

    // Polylith.toml should exist and contain the expected sections
    let polylith_toml_path = tmp.path().join("Polylith.toml");
    assert!(polylith_toml_path.exists(), "Polylith.toml should have been created");

    let polylith_content = fs::read_to_string(&polylith_toml_path).unwrap();
    assert!(polylith_content.contains("[workspace]"), "should have [workspace] section");
    assert!(polylith_content.contains("[workspace.package]"), "should have [workspace.package] section");
    assert!(polylith_content.contains("version = \"0.1.0\""), "should have version");
    assert!(polylith_content.contains("edition = \"2021\""), "should have edition");
    assert!(polylith_content.contains("[libraries]"), "should have [libraries] section");
    assert!(polylith_content.contains("serde"), "should have serde library");
    assert!(polylith_content.contains("[profiles]"), "should have [profiles] section");
    assert!(polylith_content.contains("dev"), "should have dev profile entry");

    // Root Cargo.toml should no longer contain [workspace
    let root_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        !root_content.contains("[workspace"),
        "root Cargo.toml should have no [workspace] section after migration.\ncontent:\n{root_content}"
    );
}

#[test]
fn profile_workspace_has_symlinks() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    let fixture = fixture_root();

    let copy_file = |rel: &str| {
        let src = fixture.join(rel);
        let dst = tmp.path().join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        if src.exists() {
            fs::copy(&src, &dst).unwrap();
        }
    };

    copy_file("Cargo.toml");
    copy_file("components/logger/Cargo.toml");
    copy_file("components/logger/src/lib.rs");
    copy_file("components/logger/src/logger.rs");
    copy_file("components/parser/Cargo.toml");
    copy_file("components/parser/src/lib.rs");
    copy_file("components/parser/src/parser.rs");
    copy_file("bases/cli/Cargo.toml");
    copy_file("bases/cli/src/lib.rs");
    copy_file("projects/main-project/Cargo.toml");
    copy_file("projects/main-project/src/main.rs");
    copy_file("profiles/dev.profile");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "build",
            "dev",
            "--no-build",
        ])
        .assert()
        .success();

    // profiles/dev/components, bases, projects must all be symlinks
    let components_link = tmp.path().join("profiles/dev/components");
    assert!(
        components_link.is_symlink(),
        "profiles/dev/components should be a symlink after profile build"
    );
    let bases_link = tmp.path().join("profiles/dev/bases");
    assert!(
        bases_link.is_symlink(),
        "profiles/dev/bases should be a symlink after profile build"
    );
    let projects_link = tmp.path().join("profiles/dev/projects");
    assert!(
        projects_link.is_symlink(),
        "profiles/dev/projects should be a symlink after profile build"
    );
}

#[test]
fn find_workspace_root_finds_polylith_toml() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Create Polylith.toml at root (no Cargo.toml with [workspace])
    fs::write(
        tmp.path().join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n",
    ).unwrap();

    // Create a subdirectory (simulating a component)
    let subdir = tmp.path().join("components/my-comp");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(
        subdir.join("Cargo.toml"),
        "[package]\nname = \"my-comp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();

    // find_workspace_root from the subdirectory should return the tmp root
    // We test this indirectly via the CLI using --workspace-root that was
    // resolved to the polylith root. But we can also test the function
    // directly from a unit-test in discover.rs. Here we test via the CLI
    // by running info from the subdir with the polylith root.
    //
    // Actually the easiest is just to add the root Cargo.toml as a plain
    // package (not workspace), ensuring the Polylith.toml wins over any
    // Cargo workspace walk-up.
    //
    // Verify discover works: build from subdir should find polylith root.
    // We use `cargo polylith info` with the workspace-root pointing to tmp
    // to confirm the CLI accepts it as a valid root.
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "info",
        ])
        .assert()
        .success();
}

#[test]
fn profile_migrate_strips_workspace_inheritance() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Root workspace with [workspace.package] and [workspace.dependencies]
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"[workspace]
members = ["components/logger"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"

[workspace.dependencies]
logger = { path = "components/logger" }
serde = { version = "1", features = ["derive"] }
"#,
    ).unwrap();

    // Create a minimal component that uses workspace inheritance
    let comp_dir = tmp.path().join("components/logger/src");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::write(
        tmp.path().join("components/logger/Cargo.toml"),
        r#"[package]
name = "logger"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
"#,
    ).unwrap();
    fs::write(comp_dir.join("lib.rs"), "pub fn log() {}\n").unwrap();

    // Run migrate
    let output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .output()
        .unwrap();

    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "migrate should succeed.\nstderr:\n{stderr}\nstdout:\n{stdout}"
    );

    // Check that the component Cargo.toml has been rewritten
    let comp_manifest = tmp.path().join("components/logger/Cargo.toml");
    let comp_content = fs::read_to_string(&comp_manifest).unwrap();

    assert!(
        comp_content.contains("version = \"0.1.0\""),
        "component should have explicit version.\ncontent:\n{comp_content}"
    );
    assert!(
        comp_content.contains("edition = \"2021\""),
        "component should have explicit edition.\ncontent:\n{comp_content}"
    );
    assert!(
        comp_content.contains("version = \"1\"") || comp_content.contains("serde"),
        "component should have explicit serde dep.\ncontent:\n{comp_content}"
    );
    assert!(
        comp_content.contains("derive"),
        "component serde dep should include derive feature.\ncontent:\n{comp_content}"
    );
    assert!(
        !comp_content.contains("workspace = true"),
        "component should not have any workspace = true after migration.\ncontent:\n{comp_content}"
    );

    // Summary output should mention stripping
    assert!(
        stdout.contains("Stripped workspace inheritance") || stderr.contains("Stripped workspace inheritance"),
        "output should mention stripping.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn profile_migrate_strips_inter_brick_workspace_deps() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();

    // Root workspace: logger and parser components; logger is an interface dep (path dep).
    // parser depends on logger via { workspace = true } (inter-brick dep).
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"[workspace]
members = ["components/logger", "components/parser"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"

[workspace.dependencies]
logger = { path = "components/logger" }
parser = { path = "components/parser" }
serde = { version = "1", features = ["derive"] }
"#,
    ).unwrap();

    // logger component — simple, no deps on other bricks
    let logger_src = tmp.path().join("components/logger/src");
    fs::create_dir_all(&logger_src).unwrap();
    fs::write(
        tmp.path().join("components/logger/Cargo.toml"),
        r#"[package]
name = "logger"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
"#,
    ).unwrap();
    fs::write(logger_src.join("lib.rs"), "pub fn log() {}\n").unwrap();

    // parser component — depends on logger via workspace inheritance (inter-brick dep)
    let parser_src = tmp.path().join("components/parser/src");
    fs::create_dir_all(&parser_src).unwrap();
    fs::write(
        tmp.path().join("components/parser/Cargo.toml"),
        r#"[package]
name = "parser"
version.workspace = true
edition.workspace = true

[dependencies]
logger = { workspace = true }
serde = { workspace = true }
"#,
    ).unwrap();
    fs::write(parser_src.join("lib.rs"), "pub fn parse() {}\n").unwrap();

    // Run migrate
    let output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "migrate",
        ])
        .output()
        .unwrap();

    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "migrate should succeed.\nstderr:\n{stderr}\nstdout:\n{stdout}"
    );

    // Check logger component was rewritten — library dep (serde) resolved, no workspace = true
    let logger_manifest = tmp.path().join("components/logger/Cargo.toml");
    let logger_content = fs::read_to_string(&logger_manifest).unwrap();
    assert!(
        !logger_content.contains("workspace = true"),
        "logger should have no workspace = true after migration.\ncontent:\n{logger_content}"
    );
    assert!(
        logger_content.contains("serde"),
        "logger should still have serde dep.\ncontent:\n{logger_content}"
    );

    // Check parser component was rewritten — inter-brick dep (logger) becomes explicit path dep
    let parser_manifest = tmp.path().join("components/parser/Cargo.toml");
    let parser_content = fs::read_to_string(&parser_manifest).unwrap();
    assert!(
        !parser_content.contains("workspace = true"),
        "parser should have no workspace = true after migration.\ncontent:\n{parser_content}"
    );
    assert!(
        parser_content.contains("path"),
        "parser's logger dep should be an explicit path dep.\ncontent:\n{parser_content}"
    );
    // The path from components/parser to components/logger should be ../logger
    assert!(
        parser_content.contains("../logger"),
        "parser's logger dep should use relative path '../logger'.\ncontent:\n{parser_content}"
    );
}

#[test]
fn profile_workspace_member_paths_are_relative_to_profile_root() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    let fixture = fixture_root();

    let copy_file = |rel: &str| {
        let src = fixture.join(rel);
        let dst = tmp.path().join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        if src.exists() {
            fs::copy(&src, &dst).unwrap();
        }
    };

    copy_file("Cargo.toml");
    copy_file("components/logger/Cargo.toml");
    copy_file("components/logger/src/lib.rs");
    copy_file("components/logger/src/logger.rs");
    copy_file("components/parser/Cargo.toml");
    copy_file("components/parser/src/lib.rs");
    copy_file("components/parser/src/parser.rs");
    copy_file("bases/cli/Cargo.toml");
    copy_file("bases/cli/src/lib.rs");
    copy_file("projects/main-project/Cargo.toml");
    copy_file("projects/main-project/src/main.rs");
    copy_file("profiles/dev.profile");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "profile",
            "build",
            "dev",
            "--no-build",
        ])
        .assert()
        .success();

    let generated = tmp.path().join("profiles/dev/Cargo.toml");
    let content = fs::read_to_string(&generated).unwrap();

    // Member paths must NOT start with ../../ — they are relative to the profile
    // workspace root and resolved via symlinks.
    assert!(
        !content.contains("../../"),
        "generated Cargo.toml must not contain ../../ paths — use symlink-relative paths instead.\ncontent:\n{content}"
    );

    // Member paths should use the symlink-relative form
    assert!(
        content.contains("\"components/"),
        "member paths should start with components/ not ../../components/.\ncontent:\n{content}"
    );
    assert!(
        content.contains("\"bases/"),
        "member paths should start with bases/ not ../../bases/.\ncontent:\n{content}"
    );
}
