use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Set up a pure Polylith.toml workspace (root Cargo.toml has only [package], no [workspace]).
/// This mirrors a workspace after `cargo polylith migrate` has run.
#[allow(dead_code)]
fn init_polylith_toml_workspace(dir: &TempDir) {
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"root\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("Polylith.toml"),
        "[workspace]\nname = \"my-ws\"\n\n[package]\nauthor = \"test\"\n",
    )
    .unwrap();
    // Create minimal directory structure expected by scaffold
    fs::create_dir_all(dir.path().join("components")).unwrap();
    fs::create_dir_all(dir.path().join("bases")).unwrap();
    fs::create_dir_all(dir.path().join("projects")).unwrap();
    fs::create_dir_all(dir.path().join("development")).unwrap();
}

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
fn component_lib_rs_has_mod_and_commented_reexport() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "my_comp"])
        .current_dir(dir.path())
        .assert()
        .success();

    let lib = fs::read_to_string(dir.path().join("components/my_comp/src/lib.rs")).unwrap();
    assert!(lib.contains("mod my_comp"), "lib.rs missing mod declaration");
    assert!(!lib.contains("pub use my_comp::*"), "lib.rs must not contain wildcard re-export");
    assert!(lib.contains("// pub use my_comp::"), "lib.rs missing commented re-export hint");
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
    assert!(base.join("src/lib.rs").exists(), "src/lib.rs missing");
    assert!(!base.join("src/main.rs").exists(), "src/main.rs must not be generated for base");
}

#[test]
fn base_cargo_toml_has_no_bin_section() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "base", "new", "my_base"])
        .current_dir(dir.path())
        .assert()
        .success();

    let cargo_toml =
        fs::read_to_string(dir.path().join("bases/my_base/Cargo.toml")).unwrap();
    assert!(!cargo_toml.contains("[[bin]]"), "base Cargo.toml must not contain [[bin]] section");
    assert!(!cargo_toml.contains("main.rs"), "base Cargo.toml must not reference main.rs");
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
    assert!(content.contains("[package]"), "missing [package] section");
    assert!(content.contains("[[bin]]"), "missing [[bin]] section");
}

#[test]
fn project_new_adds_to_root_workspace_members() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "project", "new", "foo"])
        .current_dir(dir.path())
        .assert()
        .success();

    let root_toml = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        root_toml.contains("projects/foo"),
        "root Cargo.toml missing workspace member for project: {root_toml}"
    );
}

#[test]
fn project_new_creates_src_main_rs() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "project", "new", "foo"])
        .current_dir(dir.path())
        .assert()
        .success();

    let main_rs = dir.path().join("projects/foo/src/main.rs");
    assert!(main_rs.exists(), "projects/foo/src/main.rs missing");
}

#[test]
fn project_new_has_no_workspace_section() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "project", "new", "foo"])
        .current_dir(dir.path())
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("projects/foo/Cargo.toml")).unwrap();
    assert!(
        !content.contains("[workspace]"),
        "project Cargo.toml should not contain [workspace] section: {content}"
    );
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

#[test]
fn project_new_creates_bin_crate_without_workspace() {
    let dir = TempDir::new().unwrap();
    init_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "project", "new", "myapp"])
        .current_dir(dir.path())
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("projects/myapp/Cargo.toml")).unwrap();
    assert!(
        !content.contains("[workspace]"),
        "project Cargo.toml must not contain [workspace] section: {content}"
    );
    assert!(
        content.contains("[[bin]]"),
        "project Cargo.toml must contain [[bin]] section: {content}"
    );
}

// ── Polylith.toml workspace (no [workspace] in root Cargo.toml) ───────────────

#[test]
fn component_new_succeeds_in_polylith_toml_workspace() {
    // Regression test for: `cargo polylith component new` fails with
    // "'workspace.members' is not an array" in Polylith.toml workspaces
    // where root Cargo.toml has no [workspace] section.
    let dir = TempDir::new().unwrap();
    init_polylith_toml_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "my_comp"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn component_new_does_not_corrupt_root_cargo_toml_in_polylith_toml_workspace() {
    let dir = TempDir::new().unwrap();
    init_polylith_toml_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "component", "new", "my_comp"])
        .current_dir(dir.path())
        .assert()
        .success();

    let root_toml = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        !root_toml.contains("[workspace]"),
        "root Cargo.toml must not gain a [workspace] section in a Polylith.toml workspace: {root_toml}"
    );
    assert!(
        !root_toml.contains("components/my_comp"),
        "root Cargo.toml must not list members in a Polylith.toml workspace: {root_toml}"
    );
}

#[test]
fn base_new_succeeds_in_polylith_toml_workspace() {
    let dir = TempDir::new().unwrap();
    init_polylith_toml_workspace(&dir);

    cargo_polylith()
        .args(["polylith", "base", "new", "my_base"])
        .current_dir(dir.path())
        .assert()
        .success();

    let root_toml = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        !root_toml.contains("[workspace]"),
        "root Cargo.toml must not gain a [workspace] section: {root_toml}"
    );
}
