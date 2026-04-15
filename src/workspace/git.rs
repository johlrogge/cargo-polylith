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
    use super::extract_version_from_cargo_toml_content;

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
