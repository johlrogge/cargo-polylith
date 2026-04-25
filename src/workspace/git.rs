use std::path::Path;
use std::process::Command;

use super::error::WorkspaceError;

/// Find the latest release tag matching the given prefix.
/// Uses `git tag --list --sort=-v:refname` and validates with semver.
/// Returns None if no matching tags exist (first release scenario).
pub fn find_last_release_tag(root: &Path, tag_prefix: &str) -> Result<Option<String>, WorkspaceError> {
    let output = Command::new("git")
        .args(["tag", "--list", &format!("{tag_prefix}*"), "--sort=-v:refname"])
        .current_dir(root)
        .output()
        .map_err(|e| WorkspaceError::Other(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_str = stderr.trim();
        // "not a git repository" means there's no git history — treat as no prior tag.
        // Any other failure (permissions, corrupt repo, etc.) is a real error.
        if stderr_str.contains("not a git repository") {
            return Ok(None);
        }
        return Err(WorkspaceError::Other(format!(
            "git tag --list failed: {stderr_str}"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Find the first tag that is a valid semver (after stripping prefix)
    for line in stdout.lines() {
        let tag = line.trim();
        if tag.is_empty() {
            continue;
        }
        let version_str = tag.strip_prefix(tag_prefix).unwrap_or(tag);
        if semver::Version::parse(version_str).is_ok() {
            return Ok(Some(tag.to_string()));
        }
    }
    Ok(None)
}

/// Read a file's content at a specific git ref.
/// Returns None if the file didn't exist at that ref.
pub fn read_file_at_ref(root: &Path, ref_name: &str, relative_path: &str) -> Result<Option<String>, WorkspaceError> {
    let output = Command::new("git")
        .args(["show", &format!("{ref_name}:{relative_path}")])
        .current_dir(root)
        .output()
        .map_err(|e| WorkspaceError::Other(format!("failed to run git show: {e}")))?;

    if !output.status.success() {
        return Ok(None); // file didn't exist at that ref
    }

    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// Extract [package] version from Cargo.toml content string.
/// Uses toml_edit (not cargo_toml) because this content comes from git show
/// and cargo_toml would try to resolve workspace inheritance.
pub fn extract_version_from_cargo_toml_content(content: &str) -> Option<String> {
    let doc: toml_edit::DocumentMut = content.parse().ok()?;
    doc.get("package")?
        .get("version")?
        .as_str()
        .map(|s| s.to_string())
}

/// Get the current git branch name. Returns None for detached HEAD.
pub fn current_branch(root: &Path) -> Result<Option<String>, WorkspaceError> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| WorkspaceError::Other(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        return Ok(None); // not a git repo
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch == "HEAD" {
        Ok(None) // detached HEAD
    } else {
        Ok(Some(branch))
    }
}

/// Returns true if `relative_path` (relative to `root`) has tracked modifications
/// in the working tree or index. Untracked files do not count as dirty.
/// Returns Ok(false) when not in a git repo or git is unavailable.
pub fn is_path_dirty(root: &Path, relative_path: &str) -> Result<bool, WorkspaceError> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--", relative_path])
        .current_dir(root)
        .output()
        .map_err(|e| WorkspaceError::Other(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Ok(false);
        }
        return Err(WorkspaceError::Other(format!(
            "git status --porcelain failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("??") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Get list of files changed between a ref and HEAD.
pub fn files_changed_since_ref(root: &Path, ref_name: &str) -> Result<Vec<String>, WorkspaceError> {
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{ref_name}..HEAD")])
        .current_dir(root)
        .output()
        .map_err(|e| WorkspaceError::Other(format!("failed to run git diff: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "not a git repository" is expected in non-git dirs — return empty.
        if stderr.contains("not a git repository") {
            return Ok(Vec::new());
        }
        return Err(WorkspaceError::Other(format!(
            "git diff --name-only failed: {}",
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{extract_version_from_cargo_toml_content, is_path_dirty};
    use std::fs;
    use std::path::Path;
    use std::process::Command as StdCommand;

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

    #[test]
    fn is_path_dirty_clean_for_committed_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "t@t.com"]);
        git(root, &["config", "user.name", "T"]);
        fs::write(root.join("file.txt"), "hello").unwrap();
        git(root, &["add", "file.txt"]);
        git(root, &["commit", "-m", "init"]);
        assert!(!is_path_dirty(root, "file.txt").unwrap());
    }

    #[test]
    fn is_path_dirty_dirty_for_unstaged_modification() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "t@t.com"]);
        git(root, &["config", "user.name", "T"]);
        fs::write(root.join("file.txt"), "hello").unwrap();
        git(root, &["add", "file.txt"]);
        git(root, &["commit", "-m", "init"]);
        // Modify without staging
        fs::write(root.join("file.txt"), "world").unwrap();
        assert!(is_path_dirty(root, "file.txt").unwrap());
    }

    #[test]
    fn is_path_dirty_dirty_for_staged_modification() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "t@t.com"]);
        git(root, &["config", "user.name", "T"]);
        fs::write(root.join("file.txt"), "hello").unwrap();
        git(root, &["add", "file.txt"]);
        git(root, &["commit", "-m", "init"]);
        // Modify and stage
        fs::write(root.join("file.txt"), "staged change").unwrap();
        git(root, &["add", "file.txt"]);
        assert!(is_path_dirty(root, "file.txt").unwrap());
    }

    #[test]
    fn is_path_dirty_clean_for_untracked_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "t@t.com"]);
        git(root, &["config", "user.name", "T"]);
        // Write but never add — untracked
        fs::write(root.join("untracked.txt"), "hello").unwrap();
        assert!(!is_path_dirty(root, "untracked.txt").unwrap());
    }

    #[test]
    fn is_path_dirty_ok_false_outside_git_repo() {
        let dir = tempfile::TempDir::new().unwrap();
        // No git init
        assert!(!is_path_dirty(dir.path(), "any_file.txt").unwrap());
    }

    #[test]
    fn is_path_dirty_ok_false_for_nonexistent_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "t@t.com"]);
        git(root, &["config", "user.name", "T"]);
        // Path does not exist — should be clean (not tracked)
        assert!(!is_path_dirty(root, "does_not_exist.txt").unwrap());
    }

    #[test]
    fn extract_version_standard_cargo_toml() {
        let content = r#"
[package]
name = "my-crate"
version = "1.2.3"
edition = "2021"
"#;
        assert_eq!(
            extract_version_from_cargo_toml_content(content),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn extract_version_no_version_field() {
        let content = r#"
[package]
name = "my-crate"
edition = "2021"
"#;
        assert_eq!(extract_version_from_cargo_toml_content(content), None);
    }

    #[test]
    fn extract_version_invalid_toml() {
        let content = "this is not valid toml :::";
        assert_eq!(extract_version_from_cargo_toml_content(content), None);
    }

    #[test]
    fn extract_version_workspace_inherited() {
        // workspace = true is not a string — should return None gracefully
        let content = r#"
[package]
name = "my-crate"
version.workspace = true
"#;
        assert_eq!(extract_version_from_cargo_toml_content(content), None);
    }
}
