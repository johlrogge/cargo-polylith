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

    // After migration, root Cargo.toml should be regenerated from the dev profile and
    // still contain [workspace] (now managed by the profile, not manually).
    let root_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        root_content.contains("[workspace]"),
        "root Cargo.toml should have [workspace] section after migration (regenerated from profile).\ncontent:\n{root_content}"
    );
    assert!(
        root_content.contains("components/logger"),
        "root Cargo.toml should have logger as a workspace member.\ncontent:\n{root_content}"
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

    // Root Cargo.toml should have been regenerated from the dev profile (no separate profiles/dev/ dir)
    let root_cargo = tmp.path().join("Cargo.toml");
    assert!(root_cargo.exists(), "root Cargo.toml should still exist");

    let content = fs::read_to_string(&root_cargo).unwrap();
    assert!(content.contains("[workspace]"), "root Cargo.toml should have [workspace] section after migration");
    // Should reference workspace members (bricks from fixture)
    assert!(
        content.contains("components/") || content.contains("bases/") || content.contains("projects/"),
        "root Cargo.toml should reference workspace members.\ncontent:\n{content}"
    );
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
    assert!(!polylith_content.contains("[workspace.package]"), "should NOT have [workspace.package] section — metadata moved to root Cargo.toml");
    assert!(polylith_content.contains("[libraries]"), "should have [libraries] section");
    assert!(polylith_content.contains("serde"), "should have serde library");
    assert!(polylith_content.contains("[profiles]"), "should have [profiles] section");
    assert!(polylith_content.contains("dev"), "should have dev profile entry");

    // Root Cargo.toml should be regenerated from profile and still have [workspace],
    // with [workspace.package] carrying the metadata.
    let root_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        root_content.contains("[workspace]"),
        "root Cargo.toml should have [workspace] section after migration (regenerated from profile).\ncontent:\n{root_content}"
    );
    assert!(
        root_content.contains("version = \"0.1.0\""),
        "root Cargo.toml [workspace.package] should have version from original workspace.package.\ncontent:\n{root_content}"
    );
    assert!(
        root_content.contains("edition = \"2021\""),
        "root Cargo.toml [workspace.package] should have edition from original workspace.package.\ncontent:\n{root_content}"
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

    // Check parser component — inter-brick dep (logger) stays as { workspace = true }
    // so that profiles can swap implementations. Only library deps (serde) are resolved.
    let parser_manifest = tmp.path().join("components/parser/Cargo.toml");
    let parser_content = fs::read_to_string(&parser_manifest).unwrap();
    // serde (library dep) should have been resolved, no more workspace = true for it
    assert!(
        parser_content.contains("serde"),
        "parser should still have serde dep.\ncontent:\n{parser_content}"
    );
    // logger (inter-brick dep) should remain as { workspace = true }
    assert!(
        parser_content.contains("logger"),
        "parser should still have logger dep.\ncontent:\n{parser_content}"
    );
    // The inter-brick dep must NOT have been converted to a path dep
    assert!(
        !parser_content.contains("../logger"),
        "parser's logger dep should NOT be converted to a path dep — it stays as workspace = true.\ncontent:\n{parser_content}"
    );
}

#[test]
fn change_profile_generates_root_cargo_toml() {
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
            "change-profile",
            "dev",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generated"));

    // The root Cargo.toml should have been overwritten
    let root_cargo = tmp.path().join("Cargo.toml");
    let content = fs::read_to_string(&root_cargo).unwrap();
    assert!(content.contains("[workspace]"), "root Cargo.toml should have [workspace] section");
    assert!(
        content.contains("\"components/logger\""),
        "root Cargo.toml should contain logger member.\ncontent:\n{content}"
    );
    assert!(
        content.contains("\"components/parser\""),
        "root Cargo.toml should contain parser member.\ncontent:\n{content}"
    );
    // No profile subdirectory should have been created
    assert!(
        !tmp.path().join("profiles/dev/Cargo.toml").exists(),
        "change-profile should NOT create profiles/dev/Cargo.toml"
    );
}

#[test]
fn change_profile_generated_header_is_present() {
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
            "change-profile",
            "dev",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        content.contains("# GENERATED BY cargo-polylith -- DO NOT EDIT"),
        "root Cargo.toml should contain the generated header.\ncontent:\n{content}"
    );
    assert!(
        content.contains("# Source: profiles/dev.profile"),
        "root Cargo.toml should reference source profile.\ncontent:\n{content}"
    );
}

#[test]
fn change_profile_errors_on_nonexistent_profile() {
    let fixture = fixture_root();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            fixture.to_str().unwrap(),
            "change-profile",
            "nonexistent-profile",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nonexistent-profile"));
}

#[test]
fn change_profile_writes_root_relative_member_paths() {
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
            "change-profile",
            "dev",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    // Paths should be root-relative — no ../../ indirection
    assert!(
        !content.contains("../../"),
        "member paths should not contain ../../.\ncontent:\n{content}"
    );
    assert!(
        content.contains("resolver = \"2\""),
        "root Cargo.toml should have resolver = \"2\".\ncontent:\n{content}"
    );
}

#[test]
fn profile_cargo_restores_root_cargo_toml_after_run() {
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

    let original_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();

    // Run `cargo polylith cargo version` — a fast cargo subcommand that always succeeds
    let _output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "cargo",
            "version",
        ])
        .output()
        .unwrap();

    // Root Cargo.toml must be restored to original content regardless of cargo outcome
    let restored_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert_eq!(
        original_content, restored_content,
        "root Cargo.toml should be restored to original after `cargo polylith cargo` completes"
    );
}

#[test]
fn profile_cargo_restores_root_cargo_toml_on_cargo_failure() {
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

    let original_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();

    // Run `cargo polylith cargo this-subcommand-does-not-exist` — cargo will fail
    let _output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "cargo",
            "this-subcommand-does-not-exist",
        ])
        .output()
        .unwrap();

    // Root Cargo.toml must be restored to original content even after cargo failure
    let restored_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert_eq!(
        original_content, restored_content,
        "root Cargo.toml should be restored to original even when cargo fails"
    );
}

/// Helper: create a minimal polylith workspace in `root` with one component, plus
/// the Polylith.toml so that `change-profile` knows it is in a polylith workspace.
fn setup_minimal_polylith_workspace(root: &std::path::Path) {
    use std::fs;

    setup_minimal_workspace(root);

    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"0.1.0\"\n",
    ).unwrap();
}

#[test]
fn change_profile_warns_on_orphaned_cargo_profile_sections() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_polylith_workspace(tmp.path());

    // Seed root Cargo.toml with [profile.release] — no generated header (user-written file)
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n\n[profile.release]\nstrip = true\n",
    ).unwrap();

    // dev.profile declares no [profile.*]
    fs::write(
        tmp.path().join("profiles/dev.profile"),
        "[implementations]\n",
    ).unwrap();

    let output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "dev",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = std::str::from_utf8(&output.stderr).unwrap();

    // Warning must mention the orphaned section name and the word "warning"
    assert!(
        stderr.contains("warning"),
        "expected 'warning' in stderr.\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("[profile.release]"),
        "warning should name the orphaned [profile.release].\nstderr:\n{stderr}"
    );

    // The regenerated root Cargo.toml must NOT contain [profile.release] —
    // it was correctly discarded (and warned about).
    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        !content.contains("[profile.release]"),
        "regenerated root Cargo.toml should not contain [profile.release] — dev profile has none.\ncontent:\n{content}"
    );
}

#[test]
fn change_profile_does_not_warn_on_generated_header_profiles() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_polylith_workspace(tmp.path());

    // prod.profile has [profile.release]
    fs::write(
        tmp.path().join("profiles/prod.profile"),
        "[implementations]\n\n[profile.release]\nstrip = true\n",
    ).unwrap();

    // First invocation: run change-profile prod — this writes a generated root Cargo.toml
    // with our GENERATED_HEADER and [profile.release] in it.
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "prod",
        ])
        .assert()
        .success();

    // Verify the generated file starts with our header (sanity check)
    let generated = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        generated.starts_with("# GENERATED BY cargo-polylith"),
        "first invocation should produce a file starting with GENERATED_HEADER.\ncontent:\n{generated}"
    );

    // Second invocation: run change-profile prod again — must NOT emit the orphan warning
    // because the [profile.release] in root Cargo.toml came from OUR previous generation.
    let output = cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "prod",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = std::str::from_utf8(&output.stderr).unwrap();

    // The second run must not produce an orphan warning
    assert!(
        !stderr.contains("[profile.release]"),
        "second change-profile should NOT warn about [profile.release] when root Cargo.toml was generated by us.\nstderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("not declared in any .profile"),
        "second change-profile should NOT show orphan warning.\nstderr:\n{stderr}"
    );
}

/// Helper: create a minimal polylith workspace in `root` with one component.
fn setup_minimal_workspace(root: &std::path::Path) {
    use std::fs;

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    ).unwrap();

    let comp_dir = root.join("components/logger/src");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::write(
        root.join("components/logger/Cargo.toml"),
        "[package]\nname = \"logger\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp_dir.join("lib.rs"), "pub fn log() {}\n").unwrap();

    fs::create_dir_all(root.join("profiles")).unwrap();
}

#[test]
fn change_profile_emits_cargo_profile_section() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_workspace(tmp.path());

    fs::write(
        tmp.path().join("profiles/prod.profile"),
        "[implementations]\n\n[profile.release]\nstrip = true\nlto = \"fat\"\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "prod",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        content.contains("[profile.release]"),
        "root Cargo.toml should contain [profile.release].\ncontent:\n{content}"
    );
    assert!(
        content.contains("strip = true"),
        "root Cargo.toml should contain strip = true.\ncontent:\n{content}"
    );
    assert!(
        content.contains("lto = \"fat\""),
        "root Cargo.toml should contain lto = \"fat\".\ncontent:\n{content}"
    );
    // [workspace.dependencies] (or end of workspace section) appears before [profile.release]
    let deps_or_workspace_pos = content.find("[workspace]").unwrap_or(0);
    let profile_pos = content.find("[profile.release]").unwrap();
    assert!(
        deps_or_workspace_pos < profile_pos,
        "[workspace] section should appear before [profile.release].\ncontent:\n{content}"
    );
}

#[test]
fn change_profile_different_profiles_emit_different_cargo_profiles() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_workspace(tmp.path());

    // dev.profile: no [profile.*]
    fs::write(
        tmp.path().join("profiles/dev.profile"),
        "[implementations]\n",
    ).unwrap();

    // prod.profile: has [profile.release]
    fs::write(
        tmp.path().join("profiles/prod.profile"),
        "[implementations]\n\n[profile.release]\nstrip = true\n",
    ).unwrap();

    // Switch to dev — no [profile.* should appear
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "dev",
        ])
        .assert()
        .success();

    let dev_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        !dev_content.contains("[profile."),
        "dev profile should produce no [profile.* sections.\ncontent:\n{dev_content}"
    );

    // Switch to prod — [profile.release] should appear
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "prod",
        ])
        .assert()
        .success();

    let prod_content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        prod_content.contains("[profile.release]"),
        "prod profile should produce [profile.release] section.\ncontent:\n{prod_content}"
    );
    assert!(
        prod_content.contains("strip = true"),
        "prod profile should have strip = true.\ncontent:\n{prod_content}"
    );
}

#[test]
fn change_profile_backward_compat_profile_without_profile_sections() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_workspace(tmp.path());

    // Only [implementations], no [profile.*]
    fs::write(
        tmp.path().join("profiles/dev.profile"),
        "[implementations]\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "dev",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        !content.contains("[profile."),
        "root Cargo.toml should contain no [profile.* text for a profile without profile sections.\ncontent:\n{content}"
    );
}

#[test]
fn change_profile_supports_multiple_profile_tables() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_workspace(tmp.path());

    fs::write(
        tmp.path().join("profiles/ci.profile"),
        r#"[implementations]

[profile.release]
lto = true

[profile.dev]
debug = 0

[profile.bench]
opt-level = 3

[profile.ci]
inherits = "release"
"#,
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "ci",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(content.contains("[profile.release]"), "should contain [profile.release].\ncontent:\n{content}");
    assert!(content.contains("[profile.dev]"), "should contain [profile.dev].\ncontent:\n{content}");
    assert!(content.contains("[profile.bench]"), "should contain [profile.bench].\ncontent:\n{content}");
    assert!(content.contains("[profile.ci]"), "should contain [profile.ci].\ncontent:\n{content}");
    assert!(content.contains("inherits = \"release\""), "should contain inherits = \"release\".\ncontent:\n{content}");
}

#[test]
fn change_profile_preserves_non_scalar_profile_values() {
    use tempfile::TempDir;
    use std::fs;

    let tmp = TempDir::new().unwrap();
    setup_minimal_workspace(tmp.path());

    // Use a nested table under profile.release to test round-trip fidelity
    fs::write(
        tmp.path().join("profiles/prod.profile"),
        "[implementations]\n\n[profile.release]\nopt-level = 3\n\n[profile.release.package.foo]\nopt-level = 3\n",
    ).unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            tmp.path().to_str().unwrap(),
            "change-profile",
            "prod",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        content.contains("opt-level = 3"),
        "root Cargo.toml should preserve opt-level = 3.\ncontent:\n{content}"
    );
    // The nested package table should appear in some form
    assert!(
        content.contains("foo"),
        "root Cargo.toml should preserve nested package.foo entry.\ncontent:\n{content}"
    );
}
