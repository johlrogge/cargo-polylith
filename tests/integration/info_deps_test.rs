use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/poly-ws")
}

// ── info ──────────────────────────────────────────────────────────────────────

#[test]
fn info_shows_components() {
    cargo_polylith()
        .args(["polylith", "info"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("logger"))
        .stdout(predicate::str::contains("parser"));
}

#[test]
fn info_shows_bases() {
    cargo_polylith()
        .args(["polylith", "info"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("cli"));
}

#[test]
fn info_shows_projects() {
    cargo_polylith()
        .args(["polylith", "info"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("main-project"));
}

#[test]
fn info_shows_section_headers() {
    cargo_polylith()
        .args(["polylith", "info"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("Components"))
        .stdout(predicate::str::contains("Bases"))
        .stdout(predicate::str::contains("Projects"));
}

// ── deps ──────────────────────────────────────────────────────────────────────

#[test]
fn deps_shows_project() {
    cargo_polylith()
        .args(["polylith", "deps"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("main-project"))
        .stdout(predicate::str::contains("(project)"));
}

#[test]
fn deps_project_shows_base_dep() {
    cargo_polylith()
        .args(["polylith", "deps"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("cli"));
}

#[test]
fn deps_shows_base_and_components() {
    cargo_polylith()
        .args(["polylith", "deps"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("cli"));
}

#[test]
fn deps_filter_by_component() {
    // --component parser: only bases that depend on parser should appear
    cargo_polylith()
        .args(["polylith", "deps", "--component", "parser"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::contains("cli"));
}

#[test]
fn deps_filter_excludes_unrelated() {
    // If we filter for a non-existent component nothing should appear
    cargo_polylith()
        .args(["polylith", "deps", "--component", "nonexistent"])
        .current_dir(fixture_root())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
