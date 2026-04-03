use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

/// Set up a minimal polylith workspace with a Polylith.toml that has versioning configured.
fn setup_relaxed_workspace(dir: &TempDir, initial_version: &str) {
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    fs::write(
        dir.path().join("Polylith.toml"),
        format!(
            "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"{initial_version}\"\n"
        ),
    )
    .unwrap();
}

/// Read the version from `Polylith.toml` `[versioning] version`.
fn read_polylith_version(dir: &TempDir) -> String {
    let content = fs::read_to_string(dir.path().join("Polylith.toml")).unwrap();
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("version = ") {
            return rest.trim().trim_matches('"').to_string();
        }
    }
    panic!("version not found in Polylith.toml: {content}");
}

#[test]
fn bump_patch_increments_version() {
    let dir = TempDir::new().unwrap();
    setup_relaxed_workspace(&dir, "0.1.0");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0").and(predicate::str::contains("0.1.1")));

    assert_eq!(read_polylith_version(&dir), "0.1.1");
}

#[test]
fn bump_minor_resets_patch() {
    let dir = TempDir::new().unwrap();
    setup_relaxed_workspace(&dir, "0.1.5");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "minor",
        ])
        .assert()
        .success();

    assert_eq!(read_polylith_version(&dir), "0.2.0");
}

#[test]
fn bump_major_resets_minor_patch() {
    let dir = TempDir::new().unwrap();
    setup_relaxed_workspace(&dir, "0.9.3");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "major",
        ])
        .assert()
        .success();

    assert_eq!(read_polylith_version(&dir), "1.0.0");
}

#[test]
fn bump_fails_without_versioning() {
    let dir = TempDir::new().unwrap();

    // Workspace without Polylith.toml
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Polylith.toml not found").or(
            predicate::str::contains("init"),
        ));
}

#[test]
fn bump_fails_in_strict_mode_with_clear_error() {
    let dir = TempDir::new().unwrap();

    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    // Strict versioning policy — bump does not support it yet.
    fs::write(
        dir.path().join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"strict\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("strict").and(predicate::str::contains("relaxed")));
}

#[test]
fn bump_fails_without_versioning_section_with_clear_error() {
    let dir = TempDir::new().unwrap();

    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    // Polylith.toml exists but has no [versioning] section.
    fs::write(
        dir.path().join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("versioning not configured")
                .or(predicate::str::contains("[versioning]")),
        );
}

#[test]
fn bump_fails_with_invalid_level() {
    let dir = TempDir::new().unwrap();
    setup_relaxed_workspace(&dir, "0.1.0");

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "micro",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown bump level").or(
            predicate::str::contains("micro"),
        ));
}

#[test]
fn bump_also_updates_root_cargo_toml_workspace_package_version() {
    let dir = TempDir::new().unwrap();

    // Workspace with [workspace.package] section
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    fs::write(
        dir.path().join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            dir.path().to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .success();

    let cargo_content = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        cargo_content.contains("0.1.1"),
        "root Cargo.toml should contain new version 0.1.1, got: {cargo_content}"
    );
}
