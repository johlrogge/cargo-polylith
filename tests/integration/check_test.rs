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
    // The fixture's cli base has direct path deps on logger and parser, which now
    // produce hardwired-dep warnings (exit 0). The check should still succeed.
    cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "check"])
        .assert()
        .success();
}

#[test]
fn check_json_fixture_has_only_warning_violations() {
    let out = cargo_polylith()
        .args(["polylith", "--workspace-root", fixture_root().to_str().unwrap(), "check", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = std::str::from_utf8(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).expect("not valid JSON");
    let violations = parsed["violations"].as_array().unwrap();
    // All violations from the fixture are warnings (hardwired-dep); no hard errors.
    // The cli base has path deps on logger and parser; parser has a path dep on logger.
    // Struct variants serialize as objects, so we check the "hardwired_dep" key.
    assert!(
        violations.iter().all(|v| v["kind"].get("hardwired_dep").is_some()),
        "expected only hardwired_dep warnings, got: {violations:?}"
    );
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

// ── lib.rs with content but no wildcard → passes ─────────────────────────────

#[test]
fn check_lib_rs_with_explicit_content_passes() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/mycomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"mycomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    // lib.rs with explicit re-exports from a dependency (mdma-style)
    fs::write(comp.join("src/lib.rs"), "pub use some_dep::{MyType, my_fn};\n").unwrap();

    // Should succeed — lib.rs exists with non-wildcard content
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success();
}

// ── flat lib.rs layout (no impl file) is valid ────────────────────────────────

#[test]
fn check_flat_lib_rs_without_impl_file_passes() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/flatcomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"flatcomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    // Flat layout: lib.rs IS the implementation, no src/flatcomp.rs needed
    fs::write(comp.join("src/lib.rs"), "pub struct FlatComp;\n").unwrap();

    // Should succeed — flat lib.rs layout without a named submodule is valid
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success();
}

#[test]
fn check_flat_lib_rs_with_explicit_reexport_passes() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/flatcomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"flatcomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    // Flat layout: lib.rs declares types and re-exports them explicitly
    fs::write(comp.join("src/lib.rs"), "mod flatcomp;\npub use flatcomp::FlatComp;\n").unwrap();
    fs::write(comp.join("src/flatcomp.rs"), "pub struct FlatComp;\n").unwrap();

    // Should succeed with no hard violations (only orphan warning)
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success();
}

// ── wildcard re-export is a warning, not an error ─────────────────────────────

#[test]
fn check_wildcard_reexport_is_warning_not_error() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/wildcomp");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"wildcomp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();
    fs::write(comp.join("src/lib.rs"), "mod wildcomp;\npub use wildcomp::*;\n").unwrap();
    fs::write(comp.join("src/wildcomp.rs"), "// impl\n").unwrap();

    // Should succeed (exit 0) even with wildcard re-export
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wildcard"));
}

// ── base missing lib.rs ───────────────────────────────────────────────────────

#[test]
fn check_detects_base_missing_lib_rs() {
    let tmp = init_valid_workspace();

    // Base with only main.rs and no lib.rs — should be a hard error
    let base = tmp.path().join("bases/nolib");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"nolib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("lib.rs"));
}

// ── base with main.rs is a warning, not an error ──────────────────────────────

#[test]
fn check_base_with_main_rs_is_warning() {
    let tmp = init_valid_workspace();

    // Base with both lib.rs (correct) and main.rs (violation) — warning only, exit 0
    let base = tmp.path().join("bases/withlib");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(base.join("src/main.rs"), "fn main() { withlib::run(); }\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"withlib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("base-has-main"));
}

// ── base-dep-base (valid — no violation) ──────────────────────────────────────

#[test]
fn check_allows_base_depending_on_base() {
    let tmp = init_valid_workspace();

    // base-a
    let ba = tmp.path().join("bases/base_a");
    fs::create_dir_all(ba.join("src")).unwrap();
    fs::write(ba.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        ba.join("Cargo.toml"),
        "[package]\nname = \"base_a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\n",
    ).unwrap();

    // base-b depends on base-a — this is valid in polylith
    let bb = tmp.path().join("bases/base_b");
    fs::create_dir_all(bb.join("src")).unwrap();
    fs::write(bb.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        bb.join("Cargo.toml"),
        "[package]\nname = \"base_b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [dependencies]\nbase_a = { path = \"../base_a\" }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .stdout(predicate::str::contains("base-dep-base").not());
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

// ── transitive component usage is not an orphan ───────────────────────────────

#[test]
fn check_transitive_component_is_not_orphan() {
    let tmp = init_valid_workspace();

    // leaf-comp: used only by mid-comp, not directly by any base
    let leaf = tmp.path().join("components/leaf-comp");
    fs::create_dir_all(leaf.join("src")).unwrap();
    fs::write(leaf.join("src/lib.rs"), "pub struct Leaf;\n").unwrap();
    fs::write(
        leaf.join("Cargo.toml"),
        "[package]\nname = \"leaf-comp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [package.metadata.polylith]\ninterface = \"leaf-comp\"\n",
    ).unwrap();

    // mid-comp: depends on leaf-comp, used directly by base
    let mid = tmp.path().join("components/mid-comp");
    fs::create_dir_all(mid.join("src")).unwrap();
    fs::write(mid.join("src/lib.rs"), "pub struct Mid;\n").unwrap();
    fs::write(
        mid.join("Cargo.toml"),
        "[package]\nname = \"mid-comp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [package.metadata.polylith]\ninterface = \"mid-comp\"\n\
         [dependencies]\nleaf-comp = { path = \"../leaf-comp\" }\n",
    ).unwrap();

    // base: depends on mid-comp only, has lib.rs (correct base layout)
    let base = tmp.path().join("bases/mybase");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"mybase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [dependencies]\nmid-comp = { path = \"../../components/mid-comp\" }\n",
    ).unwrap();

    // leaf-comp is reachable transitively — no orphan violation expected.
    // However, mid-comp and mybase have direct path deps on workspace components,
    // which now produce hardwired-dep warnings (exit 0). Assert success (no hard errors)
    // and that no orphan violation is reported.
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("orphan").not());
}

// ── project missing base is a warning ────────────────────────────────────────

#[test]
fn check_project_missing_base_is_warning() {
    let tmp = init_valid_workspace();

    // Project with no base dependency
    let proj = tmp.path().join("projects/standalone");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"standalone\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"standalone\"\npath = \"src/main.rs\"\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()                                          // warning → exit 0
        .stdout(predicate::str::contains("no-base"));
}

#[test]
fn check_project_with_base_dep_passes() {
    let tmp = TempDir::new().unwrap();
    // Root workspace members include projects/* so no project-not-in-workspace violation
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // A proper lib base
    let base = tmp.path().join("bases/mybase");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"mybase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ).unwrap();

    // Project depending on that base (new format: no [workspace] section)
    let proj = tmp.path().join("projects/wired");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"wired\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"wired\"\npath = \"src/main.rs\"\n\
         [dependencies]\nmybase = { path = \"../../bases/mybase\" }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No violations"));
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

// ── missing interface annotation on stub-named component ─────────────────────

#[test]
fn check_stub_without_interface_annotation_is_warning() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/my-svc-stub");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(comp.join("src/lib.rs"), "pub struct MySvc;\n").unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname=\"my-svc-stub\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("missing-interface"));
}

#[test]
fn check_stub_with_interface_annotation_passes() {
    let tmp = init_valid_workspace();

    let comp = tmp.path().join("components/my-svc-stub");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(comp.join("src/lib.rs"), "pub struct MySvc;\n").unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname=\"my-svc-stub\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\
         [package.metadata.polylith]\ninterface = \"my-svc\"\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("missing-interface").not());
}

// ── duplicate package name is a warning ──────────────────────────────────────

#[test]
fn check_duplicate_component_name_is_warning() {
    let tmp = init_valid_workspace();

    for dir in &["components/my-svc", "components/my-svc-wrong"] {
        let comp = tmp.path().join(dir);
        fs::create_dir_all(comp.join("src")).unwrap();
        fs::write(comp.join("src/lib.rs"), "pub struct Svc;\n").unwrap();
        // Both use the same package name — simulates a mis-named stub
        fs::write(
            comp.join("Cargo.toml"),
            "[package]\nname=\"my-svc\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        ).unwrap();
    }

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("duplicate-name"));
}

// ── test-project marker suppresses no-base ───────────────────────────────────

#[test]
fn check_test_project_marker_suppresses_no_base() {
    let tmp = init_valid_workspace();

    let proj = tmp.path().join("projects/bdd");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/lib.rs"), "// tests\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname=\"bdd\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\
         [package.metadata.polylith]\ntest-project = true\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no-base").not());
}

// ── ambiguous interface warning ───────────────────────────────────────────────

#[test]
fn check_ambiguous_interface_when_no_default_impl() {
    let tmp = init_valid_workspace();

    // Two components implementing "audio-output" — neither is named "audio-output"
    for (dir, pkg) in &[("audio_output_pipewire", "audio-output-pipewire"), ("audio_output_alsa", "audio-output-alsa")] {
        let comp = tmp.path().join(format!("components/{dir}"));
        fs::create_dir_all(comp.join("src")).unwrap();
        fs::write(comp.join("src/lib.rs"), "pub struct Out;\n").unwrap();
        fs::write(
            comp.join("Cargo.toml"),
            format!(
                "[package]\nname=\"{pkg}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\
                 [package.metadata.polylith]\ninterface = \"audio-output\"\n"
            ),
        ).unwrap();
    }

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("ambiguous-interface"));
}

#[test]
fn check_no_ambiguous_interface_when_default_impl_exists() {
    let tmp = init_valid_workspace();

    // "audio-output" (default) + "audio-output-stub" — the default exists, no warning
    for (dir, pkg) in &[("audio_output", "audio-output"), ("audio_output_stub", "audio-output-stub")] {
        let comp = tmp.path().join(format!("components/{dir}"));
        fs::create_dir_all(comp.join("src")).unwrap();
        fs::write(comp.join("src/lib.rs"), "pub struct Out;\n").unwrap();
        fs::write(
            comp.join("Cargo.toml"),
            format!(
                "[package]\nname=\"{pkg}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\
                 [package.metadata.polylith]\ninterface = \"audio-output\"\n"
            ),
        ).unwrap();
    }

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ambiguous-interface").not());
}

// ── project-not-in-workspace check ────────────────────────────────────────────

#[test]
fn check_project_not_in_root_workspace_is_warning() {
    let tmp = TempDir::new().unwrap();
    // Root workspace members do NOT include projects/*
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // A project that is NOT in root members
    let proj = tmp.path().join("projects/my-app");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname=\"my-app\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\
         [package.metadata.polylith]\ntest-project = true\n\
         [[bin]]\nname = \"my-app\"\npath = \"src/main.rs\"\n\
         [dependencies]\n",
    ).unwrap();

    // Should exit 0 (warning, not error) and show the tag
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-not-in-workspace"));
}

#[test]
fn check_project_in_root_workspace_no_violation() {
    let tmp = TempDir::new().unwrap();
    // Root workspace members DO include the project
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // A project that IS covered by root members
    let proj = tmp.path().join("projects/my-app");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname=\"my-app\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\
         [package.metadata.polylith]\ntest-project = true\n\
         [[bin]]\nname = \"my-app\"\npath = \"src/main.rs\"\n\
         [dependencies]\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-not-in-workspace").not());
}

// ── helper ────────────────────────────────────────────────────────────────────

/// Create a minimal but structurally valid workspace (no components/bases so no violations).
/// Uses wildcard members so any component/base added by a test is automatically covered.
fn init_valid_workspace() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }
    tmp
}

// ── dep key mismatch (hard error) ─────────────────────────────────────────────

#[test]
fn check_dep_key_mismatch_is_hard_error() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // Component whose package name uses underscores
    let comp = tmp.path().join("components/storage_primitives");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(comp.join("src/lib.rs"), "pub struct S;\n").unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"storage_primitives\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [package.metadata.polylith]\ninterface = \"storage-primitives\"\n",
    ).unwrap();

    // Base with lib.rs
    let base = tmp.path().join("bases/mybase");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"mybase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [dependencies]\nstorage_primitives = { path = \"../../components/storage_primitives\" }\n",
    ).unwrap();

    // Project uses hyphenated dep key (mismatches the underscore package name)
    let proj = tmp.path().join("projects/beacon");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"beacon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"beacon\"\npath = \"src/main.rs\"\n\
         [dependencies]\nmybase = { path = \"../../bases/mybase\" }\n\
         storage-primitives = { path = \"../../components/storage_primitives\" }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .failure()  // hard error → exit 1
        .stdout(predicate::str::contains("dep-key-mismatch"));
}

#[test]
fn check_dep_key_matching_package_name_passes() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // Component whose package name uses underscores
    let comp = tmp.path().join("components/storage_primitives");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(comp.join("src/lib.rs"), "pub struct S;\n").unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"storage_primitives\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [package.metadata.polylith]\ninterface = \"storage-primitives\"\n",
    ).unwrap();

    // Base with lib.rs
    let base = tmp.path().join("bases/mybase");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"mybase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [dependencies]\nstorage_primitives = { path = \"../../components/storage_primitives\" }\n",
    ).unwrap();

    // Project uses the correct package name as dep key
    let proj = tmp.path().join("projects/beacon");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"beacon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"beacon\"\npath = \"src/main.rs\"\n\
         [dependencies]\nmybase = { path = \"../../bases/mybase\" }\n\
         storage_primitives = { path = \"../../components/storage_primitives\" }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dep-key-mismatch").not());
}

#[test]
fn check_dep_key_mismatch_with_package_alias_passes() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // Component with underscore name
    let comp = tmp.path().join("components/storage_primitives");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(comp.join("src/lib.rs"), "pub struct S;\n").unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname = \"storage_primitives\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [package.metadata.polylith]\ninterface = \"storage-primitives\"\n",
    ).unwrap();

    // Base
    let base = tmp.path().join("bases/mybase");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"mybase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [dependencies]\nstorage_primitives = { path = \"../../components/storage_primitives\" }\n",
    ).unwrap();

    // Project uses hyphenated dep key BUT provides explicit package alias — NOT a violation
    let proj = tmp.path().join("projects/beacon");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"beacon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"beacon\"\npath = \"src/main.rs\"\n\
         [dependencies]\nmybase = { path = \"../../bases/mybase\" }\n\
         storage-primitives = { path = \"../../components/storage_primitives\", package = \"storage_primitives\" }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dep-key-mismatch").not());
}

// ── not-in-workspace-members (warning, not error) ─────────────────────────────

#[test]
fn check_component_not_in_workspace_members_is_warning() {
    let tmp = TempDir::new().unwrap();
    // Workspace lists only bases/cli explicitly — no components wildcard
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"bases/cli\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    let base = tmp.path().join("bases/cli");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "pub fn run() {}\n").unwrap();
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname=\"cli\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    ).unwrap();

    // Component exists on disk but is NOT listed in members
    let comp = tmp.path().join("components/ghost");
    fs::create_dir_all(comp.join("src")).unwrap();
    fs::write(comp.join("src/lib.rs"), "pub struct Ghost;\n").unwrap();
    fs::write(
        comp.join("Cargo.toml"),
        "[package]\nname=\"ghost\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("not-in-workspace"));
}

// ── standalone dep drift helpers ──────────────────────────────────────────────

/// Create a minimal polylith workspace where the root `[workspace.dependencies]`
/// declares `serde` with the given `ws_features` and `ws_version`, and there is
/// one project that declares the same `serde` dep with `proj_features` /
/// `proj_version`. Returns the TempDir handle.
fn init_drift_workspace(
    ws_features: &[&str],
    ws_version: &str,
    proj_features: &[&str],
    proj_version: &str,
) -> TempDir {
    let tmp = TempDir::new().unwrap();

    // Root Cargo.toml with [workspace.dependencies]
    let feat_list = |feats: &[&str]| -> String {
        let items: Vec<String> = feats.iter().map(|f| format!("\"{}\"", f)).collect();
        format!("[{}]", items.join(", "))
    };
    fs::write(
        tmp.path().join("Cargo.toml"),
        format!(
            "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n\
             [workspace.dependencies]\nserde = {{ version = \"{ws_version}\", features = {feats} }}\n",
            ws_version = ws_version,
            feats = feat_list(ws_features),
        ),
    ).unwrap();

    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }

    // Project Cargo.toml with its own [dependencies] for serde
    let proj = tmp.path().join("projects/myapp");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        format!(
            "[package]\nname = \"myapp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
             [[bin]]\nname = \"myapp\"\npath = \"src/main.rs\"\n\
             [dependencies]\nserde = {{ version = \"{proj_version}\", features = {feats} }}\n",
            proj_version = proj_version,
            feats = feat_list(proj_features),
        ),
    ).unwrap();

    tmp
}

// ── Check B: feature drift ────────────────────────────────────────────────────

#[test]
fn check_project_feature_drift_is_warning() {
    let tmp = init_drift_workspace(&["derive", "alloc"], "1.0", &["derive"], "1.0");
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("project-feature-drift"));
}

#[test]
fn check_project_feature_drift_superset_no_violation() {
    // Project has MORE features than workspace — not a violation
    let tmp = init_drift_workspace(&["derive"], "1.0", &["derive", "alloc"], "1.0");
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-feature-drift").not());
}

#[test]
fn check_project_feature_match_no_violation() {
    // Identical features → no drift
    let tmp = init_drift_workspace(&["derive", "alloc"], "1.0", &["alloc", "derive"], "1.0");
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-feature-drift").not());
}

// ── Check C: version drift ────────────────────────────────────────────────────

#[test]
fn check_project_version_drift_is_warning() {
    let tmp = init_drift_workspace(&["derive"], "1.0", &["derive"], "2.0");
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("project-version-drift"));
}

#[test]
fn check_project_version_match_no_violation() {
    let tmp = init_drift_workspace(&["derive"], "1.0", &["derive"], "1.0");
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-version-drift").not());
}

// ── edge cases ────────────────────────────────────────────────────────────────

#[test]
fn check_project_dep_only_in_project_no_violation() {
    // Dep exists in project but NOT in root workspace.dependencies → no violation
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }
    let proj = tmp.path().join("projects/myapp");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"myapp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"myapp\"\npath = \"src/main.rs\"\n\
         [dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-feature-drift").not())
        .stdout(predicate::str::contains("project-version-drift").not());
}

#[test]
fn check_workspace_true_dep_no_violation() {
    // `{ workspace = true }` dep is inherited from root → not checked for drift
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/*\", \"bases/*\", \"projects/*\"]\nresolver = \"2\"\n\
         [workspace.dependencies]\nserde = { version = \"1.0\", features = [\"derive\", \"alloc\"] }\n",
    ).unwrap();
    for d in &["components", "bases", "projects"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }
    let proj = tmp.path().join("projects/myapp");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(
        proj.join("Cargo.toml"),
        "[package]\nname = \"myapp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"myapp\"\npath = \"src/main.rs\"\n\
         [dependencies]\nserde = { workspace = true }\n",
    ).unwrap();

    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project-feature-drift").not())
        .stdout(predicate::str::contains("project-version-drift").not());
}

#[test]
fn check_project_feature_drift_disjoint_is_warning() {
    // Project has ["derive", "extra"], workspace has ["derive", "alloc"].
    // Neither is a subset of the other, but project is missing "alloc" → warning.
    let tmp = init_drift_workspace(&["derive", "alloc"], "1.0", &["derive", "extra"], "1.0");
    cargo_polylith()
        .args(["polylith", "--workspace-root", tmp.path().to_str().unwrap(), "check"])
        .assert()
        .success()  // warning → exit 0
        .stdout(predicate::str::contains("project-feature-drift"));
}
