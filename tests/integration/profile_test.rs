use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

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
        .stdout(predicate::str::contains("Generated"));

    // Verify the generated file exists
    let generated = tmp.path().join("profiles/dev/Cargo.toml");
    assert!(generated.exists(), "profiles/dev/Cargo.toml should have been generated");

    let content = fs::read_to_string(&generated).unwrap();
    assert!(content.contains("[workspace]"), "should have [workspace] section");
    assert!(content.contains("../../components/logger"), "should have logger member");
    assert!(content.contains("../../components/parser"), "should have parser member");
}
