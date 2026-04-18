use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

fn init_workspace_with_dirs() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();
    for d in &["components", "bases"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }
    // Intentionally no projects/ — should appear as a suggestion
    tmp
}

// ── missing projects/ appears as suggestion ───────────────────────────────────

#[test]
fn status_missing_projects_dir_is_suggestion() {
    let tmp = init_workspace_with_dirs();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("projects"));
}

// ── flat lib.rs layout → divergence not error ─────────────────────────────────

#[test]
fn status_flat_lib_rs_layout_is_divergence() {
    let tmp = init_workspace_with_dirs();

    let comp = tmp.path().join("components/flatcomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"flatcomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    // Flat layout: lib.rs exists but no src/flatcomp.rs
    fs::write(comp.join("src/lib.rs"), "pub struct FlatComp;\n").unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("flatcomp"));
}

// ── explicit re-exports → confirmed ───────────────────────────────────────────

#[test]
fn status_explicit_reexport_is_confirmed() {
    let tmp = init_workspace_with_dirs();

    let comp = tmp.path().join("components/mycomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"mycomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(comp.join("src/lib.rs"), "mod mycomp;\npub use mycomp::MyType;\n").unwrap();
    fs::write(comp.join("src/mycomp.rs"), "pub struct MyType;\n").unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("explicit"));
}

// ── wildcard re-exports → divergence ──────────────────────────────────────────

#[test]
fn status_wildcard_reexport_is_divergence() {
    let tmp = init_workspace_with_dirs();

    let comp = tmp.path().join("components/mycomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"mycomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(comp.join("src/lib.rs"), "mod mycomp;\npub use mycomp::*;\n").unwrap();
    fs::write(comp.join("src/mycomp.rs"), "// impl\n").unwrap();

    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.contains("wildcard") || text.contains("Divergence"), "{text}");
}

// ── old-model profile directory suggestion ────────────────────────────────────

#[test]
fn status_old_model_profile_dir_is_suggestion() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();
    let dev_dir = tmp.path().join("profiles/dev");
    fs::create_dir_all(&dev_dir).unwrap();
    fs::write(
        dev_dir.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("profiles/dev"))
        .stdout(predicate::str::contains("change-profile dev"));
}

#[test]
fn status_old_model_json_suggestion() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();
    let dev_dir = tmp.path().join("profiles/dev");
    fs::create_dir_all(&dev_dir).unwrap();
    fs::write(
        dev_dir.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    let suggestions = parsed["suggestions"].as_array().expect("suggestions must be array");
    assert!(
        suggestions.iter().any(|s| {
            let s = s.as_str().unwrap_or("");
            s.contains("profiles/dev") && s.contains("change-profile dev")
        }),
        "expected old-model hint in suggestions JSON, got: {suggestions:?}"
    );
}

// ── --json output has required keys ───────────────────────────────────────────

#[test]
fn status_json_has_required_keys() {
    let tmp = init_workspace_with_dirs();

    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    assert!(parsed["confirmed"].is_array(), "missing confirmed");
    assert!(parsed["divergences"].is_array(), "missing divergences");
    assert!(parsed["suggestions"].is_array(), "missing suggestions");
}
