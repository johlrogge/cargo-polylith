use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

/// Set up a minimal workspace in `dir` and run `cargo polylith init`.
fn init_workspace(dir: &TempDir) {
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();
    cargo_polylith()
        .args(["polylith", "init"])
        .current_dir(dir.path())
        .assert()
        .success();
}

// ── component new ─────────────────────────────────────────────────────────────

#[test]
fn component_new_creates_files() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "my_comp"])
        .current_dir(dir.path())
        .assert()
        .success();

    let base = dir.path().join("components/my_comp");
    assert!(base.join("Cargo.toml").exists(), "Cargo.toml missing");
    assert!(base.join("src/lib.rs").exists(), "src/lib.rs missing");
    assert!(base.join("src/my_comp.rs").exists(), "src/my_comp.rs missing");
}

#[test]
fn component_lib_rs_re_exports() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "my_comp"])
        .current_dir(dir.path())
        .assert()
        .success();

    let lib = fs::read_to_string(dir.path().join("components/my_comp/src/lib.rs")).unwrap();
    assert!(lib.contains("mod my_comp"), "lib.rs missing mod declaration");
    assert!(lib.contains("pub use my_comp::*"), "lib.rs missing re-export");
}

#[test]
fn component_new_adds_workspace_member() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "my_comp"])
        .current_dir(dir.path())
        .assert()
        .success();

    let root_toml = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        root_toml.contains("components/my_comp"),
        "root Cargo.toml missing workspace member: {root_toml}"
    );
}

#[test]
fn component_new_prints_success() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "widget"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("widget"));
}

#[test]
fn component_new_multiple_members_preserved() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "alpha"])
        .current_dir(dir.path())
        .assert()
        .success();

    cargo_polylith()
        .args(["polylith", "component", "new", "beta"])
        .current_dir(dir.path())
        .assert()
        .success();

    let root_toml = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(root_toml.contains("components/alpha"), "alpha missing");
    assert!(root_toml.contains("components/beta"), "beta missing");
}

// ── base new ──────────────────────────────────────────────────────────────────

#[test]
fn base_new_creates_files() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "base", "new", "my_base"])
        .current_dir(dir.path())
        .assert()
        .success();

    let base = dir.path().join("bases/my_base");
    assert!(base.join("Cargo.toml").exists(), "Cargo.toml missing");
    assert!(base.join("src/main.rs").exists(), "src/main.rs missing");
}

#[test]
fn base_cargo_toml_has_bin_section() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "base", "new", "my_base"])
        .current_dir(dir.path())
        .assert()
        .success();

    let cargo_toml =
        fs::read_to_string(dir.path().join("bases/my_base/Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("[[bin]]"), "missing [[bin]] section");
    assert!(cargo_toml.contains("main.rs"), "[[bin]] doesn't reference main.rs");
}

#[test]
fn base_new_adds_workspace_member() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "base", "new", "my_base"])
        .current_dir(dir.path())
        .assert()
        .success();

    let root_toml = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        root_toml.contains("bases/my_base"),
        "root Cargo.toml missing workspace member"
    );
}

// ── project new ───────────────────────────────────────────────────────────────

#[test]
fn project_new_creates_cargo_toml() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "project", "new", "my_project"])
        .current_dir(dir.path())
        .assert()
        .success();

    let p = dir.path().join("projects/my_project/Cargo.toml");
    assert!(p.exists(), "project Cargo.toml missing");

    let content = fs::read_to_string(&p).unwrap();
    assert!(content.contains("[workspace]"), "missing [workspace] section");
}

#[test]
fn project_new_prints_hint() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "project", "new", "my_project"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("my_project"));
}
