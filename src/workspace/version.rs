use std::fs;
use std::str::FromStr;

use super::error::WorkspaceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpLevel {
    Major,
    Minor,
    Patch,
}

impl FromStr for BumpLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "major" => Ok(BumpLevel::Major),
            "minor" => Ok(BumpLevel::Minor),
            "patch" => Ok(BumpLevel::Patch),
            other => Err(format!(
                "unknown bump level '{}' (expected 'major', 'minor', or 'patch')",
                other
            )),
        }
    }
}

pub fn compute_bumped_version(
    current: &str,
    level: BumpLevel,
) -> Result<semver::Version, WorkspaceError> {
    let mut ver = semver::Version::parse(current)
        .map_err(|e| WorkspaceError::Other(format!("invalid version '{}': {}", current, e)))?;
    match level {
        BumpLevel::Major => {
            ver.major += 1;
            ver.minor = 0;
            ver.patch = 0;
        }
        BumpLevel::Minor => {
            ver.minor += 1;
            ver.patch = 0;
        }
        BumpLevel::Patch => {
            ver.patch += 1;
        }
    }
    ver.pre = semver::Prerelease::EMPTY;
    Ok(ver)
}

/// Returns names of bricks whose Cargo.toml does NOT use `version.workspace = true`.
///
/// NOTE: This function uses `toml_edit` for reading, which is an exception to the project
/// convention of "toml_edit for writing, cargo_toml for reading". The reason is that
/// `cargo_toml` normalizes `version = { workspace = true }` away during parsing, making
/// it impossible to detect whether a brick is using workspace version inheritance.
/// `toml_edit` preserves the raw TOML structure so we can inspect the actual value.
pub fn bricks_not_using_workspace_version(bricks: &[super::model::Brick]) -> Vec<String> {
    let mut result = Vec::new();
    for brick in bricks {
        let manifest = brick.path.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&manifest) {
            if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                let uses_workspace = doc
                    .get("package")
                    .and_then(|p| p.get("version"))
                    .and_then(|v| v.as_inline_table())
                    .and_then(|t| t.get("workspace"))
                    .and_then(|w| w.as_bool())
                    .unwrap_or(false);
                if !uses_workspace {
                    result.push(brick.name.clone());
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_level_from_str_major() {
        assert_eq!("major".parse::<BumpLevel>().unwrap(), BumpLevel::Major);
    }

    #[test]
    fn bump_level_from_str_minor() {
        assert_eq!("minor".parse::<BumpLevel>().unwrap(), BumpLevel::Minor);
    }

    #[test]
    fn bump_level_from_str_patch() {
        assert_eq!("patch".parse::<BumpLevel>().unwrap(), BumpLevel::Patch);
    }

    #[test]
    fn bump_level_from_str_unknown_errors() {
        let err = "micro".parse::<BumpLevel>().unwrap_err();
        assert!(err.contains("unknown bump level"), "unexpected error: {err}");
        assert!(err.contains("micro"), "unexpected error: {err}");
    }

    #[test]
    fn compute_bumped_version_patch() {
        let v = compute_bumped_version("0.1.0", BumpLevel::Patch).unwrap();
        assert_eq!(v.to_string(), "0.1.1");
    }

    #[test]
    fn compute_bumped_version_minor() {
        let v = compute_bumped_version("0.1.5", BumpLevel::Minor).unwrap();
        assert_eq!(v.to_string(), "0.2.0");
    }

    #[test]
    fn compute_bumped_version_major() {
        let v = compute_bumped_version("0.9.3", BumpLevel::Major).unwrap();
        assert_eq!(v.to_string(), "1.0.0");
    }

    #[test]
    fn compute_bumped_version_clears_prerelease() {
        let v = compute_bumped_version("1.0.0-alpha.1", BumpLevel::Patch).unwrap();
        assert_eq!(v.to_string(), "1.0.1");
    }

    #[test]
    fn compute_bumped_version_invalid_semver_errors() {
        let err = compute_bumped_version("not-a-version", BumpLevel::Patch).unwrap_err();
        assert!(
            matches!(err, WorkspaceError::Other(_)),
            "expected Other variant"
        );
    }
}
