use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
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
fn bump_strict_mode_succeeds_and_shows_analysis() {
    let dir = TempDir::new().unwrap();

    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    // Strict versioning policy — now supported.
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
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Strict bump analysis"));
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

// ── helpers for git-based strict mode tests ───────────────────────────────────

fn git(dir: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} exited with {status}");
}

/// Set up a minimal strict-mode polylith workspace inside a git repo.
/// Creates:
///   - Polylith.toml with policy = "strict"
///   - Cargo.toml (workspace)
///   - components/foo/Cargo.toml + src/lib.rs with one pub fn
/// Returns the temp dir (caller keeps it alive).
fn setup_strict_workspace_with_git(initial_version: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // git init
    git(root, &["init"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);

    // Workspace Cargo.toml
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/foo\"]\nresolver = \"2\"\n",
    )
    .unwrap();

    // Polylith.toml
    fs::write(
        root.join("Polylith.toml"),
        format!(
            "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"strict\"\nversion = \"{initial_version}\"\n"
        ),
    )
    .unwrap();

    // Component foo
    let foo_dir = root.join("components").join("foo");
    fs::create_dir_all(foo_dir.join("src")).unwrap();
    fs::write(
        foo_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"foo\"\nversion = \"{initial_version}\"\nedition = \"2021\"\n"
        ),
    )
    .unwrap();
    fs::write(foo_dir.join("src").join("lib.rs"), "pub fn hello() {}\n").unwrap();

    // Initial commit + tag
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial commit"]);
    git(root, &["tag", &format!("v{initial_version}")]);

    dir
}

#[test]
fn strict_bump_no_tag_first_release() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // git init (no tags)
    git(root, &["init"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/foo\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"strict\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let foo_dir = root.join("components").join("foo");
    fs::create_dir_all(foo_dir.join("src")).unwrap();
    fs::write(
        foo_dir.join("Cargo.toml"),
        "[package]\nname = \"foo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(foo_dir.join("src").join("lib.rs"), "pub fn hello() {}\n").unwrap();

    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial commit"]);

    // No tag — first release scenario
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "--dry-run",
        ])
        .assert()
        .success()
        // Should print the analysis header
        .stdout(predicate::str::contains("Strict bump analysis"))
        // Should note no prior release tag
        .stderr(predicate::str::contains("no previous release tag"));
}

/// Set up a minimal relaxed-mode polylith workspace inside a git repo,
/// with an initial commit so files are tracked and clean.
fn setup_relaxed_workspace_with_git(initial_version: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    git(root, &["init"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();

    fs::write(
        root.join("Polylith.toml"),
        format!(
            "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"{initial_version}\"\n"
        ),
    )
    .unwrap();

    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial commit"]);

    dir
}

#[test]
fn bump_refuses_when_polylith_toml_is_dirty() {
    let dir = setup_relaxed_workspace_with_git("1.0.0");
    let root = dir.path();

    // Simulate a partial prior bump: write a different version directly to Polylith.toml
    // without committing (making it dirty).
    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"1.1.0\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "minor",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("refusing to bump")
                .and(predicate::str::contains("Polylith.toml"))
                .and(predicate::str::contains("--allow-dirty")),
        );

    // Version on disk should be the dirty pre-state (1.1.0), not bumped further.
    let content = fs::read_to_string(root.join("Polylith.toml")).unwrap();
    assert!(
        content.contains("1.1.0"),
        "version should remain at dirty pre-state 1.1.0, got: {content}"
    );
}

#[test]
fn bump_refuses_when_root_cargo_toml_is_dirty() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    git(root, &["init"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n\n[workspace.package]\nversion = \"1.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial commit"]);

    // Make Cargo.toml dirty (unstaged modification)
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n\n[workspace.package]\nversion = \"1.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "minor",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("refusing to bump")
                .and(predicate::str::contains("Cargo.toml")),
        );

    // Versions on disk should remain at the pre-bump state — bump was refused.
    let polylith_content = fs::read_to_string(root.join("Polylith.toml")).unwrap();
    assert!(
        polylith_content.contains("1.0.0"),
        "Polylith.toml version should remain at 1.0.0 after refused bump, got: {polylith_content}"
    );
    let cargo_content = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(
        cargo_content.contains("1.1.0"),
        "Cargo.toml should still contain the dirty pre-bump version 1.1.0, got: {cargo_content}"
    );
}

#[test]
fn bump_succeeds_when_cargo_toml_is_dirty_without_workspace_package() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    git(root, &["init"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);

    // Cargo.toml with NO [workspace.package] — only the workspace members list.
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial commit"]);

    // Make Cargo.toml dirty (unstaged modification), even though it has no [workspace.package].
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"components/new\"]\nresolver = \"2\"\n",
    )
    .unwrap();

    // The bump should SUCCEED — dirty Cargo.toml without [workspace.package] is not a concern.
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .success();

    // Polylith.toml should have been bumped.
    assert_eq!(read_polylith_version(&dir), "1.0.1");
}

#[test]
fn bump_succeeds_with_allow_dirty_override() {
    let dir = setup_relaxed_workspace_with_git("1.0.0");
    let root = dir.path();

    // Make Polylith.toml dirty at version 1.1.0 (simulating partial prior bump)
    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"1.1.0\"\n",
    )
    .unwrap();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "minor",
            "--allow-dirty",
        ])
        .assert()
        .success();

    // Should have bumped from the dirty current value (1.1.0 -> 1.2.0)
    assert_eq!(read_polylith_version(&dir), "1.2.0");
}

#[test]
fn bump_succeeds_on_clean_committed_workspace() {
    let dir = setup_relaxed_workspace_with_git("0.3.0");
    let root = dir.path();

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .success();

    assert_eq!(read_polylith_version(&dir), "0.3.1");
}

#[test]
fn bump_succeeds_when_polylith_toml_is_untracked() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // git init but never add/commit
    git(root, &["init"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Polylith.toml"),
        "[workspace]\nschema_version = 1\n\n[versioning]\npolicy = \"relaxed\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    // Deliberately do NOT git add — files are untracked

    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "patch",
        ])
        .assert()
        .success();

    assert_eq!(read_polylith_version(&dir), "0.1.1");
}

#[test]
fn strict_bump_dry_run_shows_recommendations() {
    let dir = setup_strict_workspace_with_git("0.1.0");
    let root = dir.path();

    // Now modify the component: add a new pub fn (interface change)
    let lib_rs = root.join("components").join("foo").join("src").join("lib.rs");
    fs::write(&lib_rs, "pub fn hello() {}\npub fn world() {}\n").unwrap();

    // Bump component version to signal the change
    let cargo_toml = root.join("components").join("foo").join("Cargo.toml");
    fs::write(
        &cargo_toml,
        "[package]\nname = \"foo\"\nversion = \"0.2.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    git(root, &["add", "."]);
    git(root, &["commit", "-m", "feat(foo): add world fn"]);

    // Run strict bump --dry-run
    cargo_polylith()
        .args([
            "polylith",
            "--workspace-root",
            root.to_str().unwrap(),
            "bump",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Strict bump analysis"));
}
