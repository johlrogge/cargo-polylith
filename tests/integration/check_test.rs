use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/poly-ws")
}

// ── clean fixture passes ───────────────────────────────────────────────────────

#[test]
fn check_clean_fixture_passes() {
    cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No violations"));
}

#[test]
fn check_json_clean_fixture_has_empty_violations() {
    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "check", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    assert_eq!(parsed["violations"].as_array().unwrap().len(), 0);
}

// ── missing lib.rs ────────────────────────────────────────────────────────────

#[test]
fn check_detects_missing_lib_rs() {
    let tmp = init_valid_workspace();

    // Create component without lib.rs
    let comp = tmp.path().join("components/broken");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    // Intentionally no src/lib.rs

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("lib.rs"));
}

// ── missing re-export ─────────────────────────────────────────────────────────

#[test]
fn check_detects_missing_re_export() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/mycomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"mycomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    // lib.rs exists but has no re-export
    fs::write(comp.join("src/lib.rs"), "// empty\n").unwrap();
    fs::write(comp.join("src/mycomp.rs"), "// impl\n").unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("pub use mycomp::*"));
}

// ── missing impl file ─────────────────────────────────────────────────────────

#[test]
fn check_detects_missing_impl_file() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/noimpl");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"noimpl\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp.join("src/lib.rs"), "mod noimpl;\npub use noimpl::*;\n").unwrap();
    // Intentionally no src/noimpl.rs

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("noimpl.rs"));
}

// ── missing main.rs ───────────────────────────────────────────────────────────

#[test]
fn check_detects_missing_main_rs() {
    let tmp = init_valid_workspace();

    let base = tmp.path().join("bases/nomain");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"nomain\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"nomain\"\npath = \"src/main.rs\"\n",
    ).unwrap();
    // Intentionally no src/main.rs

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("main.rs"));
}

// ── base-dep-base ─────────────────────────────────────────────────────────────

#[test]
fn check_detects_base_depending_on_base() {
    let tmp = init_valid_workspace();

    // base-a
    let ba = tmp.path().join("bases/base_a");
    fs::create_dir_all(ba.join("src")).unwrap();
    fs::write(ba.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        ba.join("Cargo.toml"),
        "[package]\nname = \"base_a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"base_a\"\npath = \"src/main.rs\"\n[dependencies]\n",
    ).unwrap();

    // base-b depends on base-a
    let bb = tmp.path().join("bases/base_b");
    fs::create_dir_all(bb.join("src")).unwrap();
    fs::write(bb.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        bb.join("Cargo.toml"),
        "[package]\nname = \"base_b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"base_b\"\npath = \"src/main.rs\"\n\
         [dependencies]\nbase_a = { path = \"../base_a\" }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("base_b"))
        .stdout(predicate::str::contains("base_a"));
}

// ── orphan component (warning, not error) ─────────────────────────────────────

#[test]
fn check_orphan_is_warning_not_error() {
    let tmp = init_valid_workspace();

    // Add a component with no base depending on it
    let comp = tmp.path().join("components/orphan");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"orphan\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp.join("src/lib.rs"), "mod orphan;\npub use orphan::*;\n").unwrap();
    fs::write(comp.join("src/orphan.rs"), "// impl\n").unwrap();

    // Should succeed (exit 0) even though there's an orphan
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("orphan"));
}

// ── check --json shows violation kind ────────────────────────────────────────

#[test]
fn check_json_shows_violation_kind() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/badcomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"badcomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    // No lib.rs

    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check", "--json"])
        .assert()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    let violations = parsed["violations"].as_array().unwrap();
    assert!(!violations.is_empty());
    assert!(violations.iter().any(|v| v["kind"] == "missing_lib_rs"), "{violations:?}");
}

// ── helper ────────────────────────────────────────────────────────────────────

/// Create a minimal but structurally valid workspace (no components/bases so no violations).
fn init_valid_workspace() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }
    tmp
}
