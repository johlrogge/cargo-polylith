/// Phase 3 integration tests:
///   - --workspace-root flag
///   - --json output for info and deps
///   - error messages for bad input / missing workspace
use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;
use std::fs;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/poly-ws")
}

// ── --workspace-root ──────────────────────────────────────────────────────────

#[test]
fn workspace_root_flag_info() {
    // Run from a temp dir but point --workspace-root at the fixture.
    let tmp = TempDir::new().unwrap();
    cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "info"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("logger"))
        .stdout(predicate::str::contains("cli"));
}

#[test]
fn workspace_root_flag_deps() {
    let tmp = TempDir::new().unwrap();
    cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "deps"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("cli"));
}

#[test]
fn workspace_root_flag_component_new() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    ).unwrap();
    cargo_polylith()
        .args(["polylith", "init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Run component new from a subdirectory, pointing root at tmp
    let sub = tmp.path().join("some_subdir");
    fs::create_dir(&sub).unwrap();
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(),
               "component", "new", "my_widget"])
        .current_dir(&sub)
        .assert()
        .success();

    assert!(tmp.path().join("components/my_widget/Cargo.toml").exists());
}

#[test]
fn workspace_root_bad_path_errors() {
    let tmp = TempDir::new().unwrap();
    cargo_polylith()
        .args(["polylith", "--workspace-root", "/nonexistent/path/xyz", "info"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ── --json ────────────────────────────────────────────────────────────────────

#[test]
fn info_json_is_valid_json() {
    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "info", "--json"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text)
        .expect("info --json output is not valid JSON");

    assert!(parsed["components"].is_array());
    assert!(parsed["bases"].is_array());
    assert!(parsed["projects"].is_array());
}

#[test]
fn info_json_contains_correct_names() {
    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "info", "--json"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();

    let comp_names: Vec<_> = parsed["components"]
        .as_array().unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(comp_names.contains(&"logger"), "{comp_names:?}");
    assert!(comp_names.contains(&"parser"), "{comp_names:?}");

    let base_names: Vec<_> = parsed["bases"]
        .as_array().unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(base_names.contains(&"cli"), "{base_names:?}");
}

#[test]
fn deps_json_is_valid_json() {
    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "deps", "--json"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text)
        .expect("deps --json output is not valid JSON");

    assert!(parsed["bases"].is_array());
}

#[test]
fn deps_json_component_filter() {
    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(),
               "deps", "--json", "--component", "parser"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    let bases = parsed["bases"].as_array().unwrap();
    // cli depends on parser, so it should appear
    assert!(bases.iter().any(|b| b["name"] == "cli"), "{bases:?}");
}

// ── error messages ────────────────────────────────────────────────────────────

#[test]
fn no_workspace_gives_clear_warning() {
    let tmp = TempDir::new().unwrap();
    cargo_polylith()
        .args(["polylith", "info"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("warning"));
}

#[test]
fn invalid_component_name_gives_error() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers=[]\nresolver=\"2\"\n",
    ).unwrap();
    cargo_polylith()
        .args(["polylith", "init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    cargo_polylith()
        .args(["polylith", "component", "new", "bad name!"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));

    cargo_polylith()
        .args(["polylith", "component", "new", "1foo"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}
