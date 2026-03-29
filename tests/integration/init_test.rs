use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cargo_polylith() -> Command {
    Command::cargo_bin("cargo-polylith").unwrap()
}

/// Run `cargo-polylith polylith init` in `dir`.
fn run_init(dir: &TempDir) -> assert_cmd::assert::Assert {
    // Seed a minimal workspace Cargo.toml so init can succeed (it doesn't
    // need a [workspace] section itself — the tool only writes dirs).
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args(["polylith", "init"])
        .current_dir(dir.path())
        .assert()
}

#[test]
fn init_creates_directories() {
    let dir = TempDir::new().unwrap();
    run_init(&dir).success();

    assert!(dir.path().join("components").is_dir(), "components/ missing");
    assert!(dir.path().join("bases").is_dir(), "bases/ missing");
    assert!(dir.path().join("projects").is_dir(), "projects/ missing");
}

#[test]
fn init_creates_cargo_config() {
    let dir = TempDir::new().unwrap();
    run_init(&dir).success();

    let config = dir.path().join(".cargo").join("config.toml");
    assert!(config.exists(), ".cargo/config.toml missing");
    let content = fs::read_to_string(&config).unwrap();
    assert!(
        content.contains("target-dir"),
        ".cargo/config.toml missing target-dir setting"
    );
}

#[test]
fn init_prints_success_message() {
    let dir = TempDir::new().unwrap();
    run_init(&dir)
        .success()
        .stdout(predicate::str::contains("Initialised polylith workspace"))
        .stdout(predicate::str::contains("Next steps"));
}

#[test]
fn init_warns_on_existing_dirs() {
    let dir = TempDir::new().unwrap();
    // Pre-create one of the dirs.
    fs::create_dir(dir.path().join("components")).unwrap();

    run_init(&dir)
        .success()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn init_idempotent_does_not_fail() {
    let dir = TempDir::new().unwrap();
    // Run twice — second run should succeed (with warnings).
    run_init(&dir).success();
    run_init(&dir).success();
}

#[test]
fn init_respects_workspace_root_flag() {
    // The target workspace lives in a subdirectory; we run the command from a
    // *different* directory and point --workspace-root at the target.
    let target = TempDir::new().unwrap();
    fs::write(
        target.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    // Run from a completely separate temp dir to confirm cwd is NOT used.
    let other = TempDir::new().unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            target.path().to_str().unwrap(),
            "init",
        ])
        .current_dir(other.path())
        .assert()
        .success();

    assert!(
        target.path().join("components").is_dir(),
        "components/ should be created inside the --workspace-root directory"
    );
    assert!(
        !other.path().join("components").exists(),
        "components/ must NOT be created in cwd when --workspace-root is given"
    );
}
