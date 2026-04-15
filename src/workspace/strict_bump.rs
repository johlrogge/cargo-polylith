use std::collections::HashMap;
use std::path::Path;

use super::api_diff::{compare_pub_api, ApiDiff};
use super::error::WorkspaceError;
use super::git;
use super::model::WorkspaceMap;
use super::version::{compute_bumped_version, BumpLevel};

/// Change severity for a single brick.
/// Ordered from least to most severe — PartialOrd/Ord is used for worst-wins aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeSeverity {
    Unchanged,
    TransitivePatch,
    InternalsChanged,
    InterfaceChanged,
}

impl ChangeSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeSeverity::Unchanged => "unchanged",
            ChangeSeverity::TransitivePatch => "transitive-patch",
            ChangeSeverity::InternalsChanged => "internals-changed",
            ChangeSeverity::InterfaceChanged => "interface-changed",
        }
    }

    /// Map a severity level to the corresponding bump level.
    /// Returns None when no version bump is needed.
    pub fn to_bump_level(self) -> Option<BumpLevel> {
        match self {
            Self::Unchanged => None,
            Self::TransitivePatch => Some(BumpLevel::Patch),
            Self::InternalsChanged => Some(BumpLevel::Minor),
            Self::InterfaceChanged => Some(BumpLevel::Major),
        }
    }
}

/// Per-brick analysis result.
#[derive(Debug, Clone)]
pub struct BrickChangeReport {
    pub severity: ChangeSeverity,
}

/// Per-project aggregate recommendation.
#[derive(Debug, Clone)]
pub struct ProjectBumpRecommendation {
    pub project_name: String,
    pub current_version: String,
    pub recommended_level: Option<BumpLevel>,
    pub recommended_version: Option<String>,
    pub worst_severity: ChangeSeverity,
    pub changed_bricks: Vec<String>,
}

/// Analyze brick changes by comparing versions and source code against a git tag.
///
/// For each brick (component + base) in the WorkspaceMap:
/// - Reads the brick's Cargo.toml at the given git tag to get old version
/// - Reads the brick's current Cargo.toml to get current version
/// - If versions differ, compares src/lib.rs to determine severity
/// - If the brick didn't exist at the tag, treats it as InterfaceChanged (new brick)
pub fn analyze_brick_changes(
    root: &Path,
    map: &WorkspaceMap,
    tag: &str,
) -> Result<HashMap<String, BrickChangeReport>, WorkspaceError> {
    let mut reports = HashMap::new();

    // Collect all bricks (components and bases)
    let all_bricks = map.components.iter().chain(map.bases.iter());

    for brick in all_bricks {
        let brick_path = brick.path.strip_prefix(root).unwrap_or(&brick.path);
        let cargo_toml_rel = brick_path.join("Cargo.toml");
        let cargo_toml_rel_str = cargo_toml_rel.to_string_lossy().replace('\\', "/");

        // Read old Cargo.toml at the tag
        let old_cargo_content =
            git::read_file_at_ref(root, tag, &cargo_toml_rel_str)?;

        // Read current Cargo.toml
        let current_cargo_path = brick.path.join("Cargo.toml");
        let current_cargo_content = std::fs::read_to_string(&current_cargo_path).map_err(|e| {
            WorkspaceError::Io {
                path: current_cargo_path.clone(),
                source: e,
            }
        })?;

        let new_version =
            git::extract_version_from_cargo_toml_content(&current_cargo_content)
                .unwrap_or_else(|| "0.0.0".to_string());

        let severity = match &old_cargo_content {
            None => {
                // Brick didn't exist at the tag — new brick
                ChangeSeverity::InterfaceChanged
            }
            Some(old_content) => {
                let old_ver = git::extract_version_from_cargo_toml_content(old_content);
                if old_ver.as_deref() == Some(new_version.as_str()) {
                    ChangeSeverity::Unchanged
                } else {
                    // Versions differ — compare source code
                    let lib_rs_rel = brick_path.join("src").join("lib.rs");
                    let lib_rs_rel_str = lib_rs_rel.to_string_lossy().replace('\\', "/");

                    let old_lib_content = git::read_file_at_ref(root, tag, &lib_rs_rel_str)?;
                    let new_lib_path = brick.path.join("src").join("lib.rs");
                    let new_lib_content = std::fs::read_to_string(&new_lib_path).ok();

                    match (old_lib_content, new_lib_content) {
                        (Some(old_src), Some(new_src)) => match compare_pub_api(&old_src, &new_src) {
                            ApiDiff::InterfaceChanged => ChangeSeverity::InterfaceChanged,
                            ApiDiff::InternalsOnly => ChangeSeverity::InternalsChanged,
                            ApiDiff::Unchanged => {
                                // Version bumped but no code change = transitive dep bump
                                ChangeSeverity::TransitivePatch
                            }
                        },
                        (None, _) => {
                            // Old lib.rs didn't exist — treat as interface change
                            ChangeSeverity::InterfaceChanged
                        }
                        (_, None) => {
                            // Current lib.rs doesn't exist — could be a base with main.rs
                            ChangeSeverity::InternalsChanged
                        }
                    }
                }
            }
        };

        reports.insert(brick.name.clone(), BrickChangeReport { severity });
    }

    Ok(reports)
}

/// Compute per-project bump recommendations based on brick change analysis.
///
/// For each project, walks the transitive dependency subtree, collects
/// ChangeSeverity values, and picks worst-wins to determine the bump level.
pub fn compute_project_recommendations(
    map: &WorkspaceMap,
    brick_changes: &HashMap<String, BrickChangeReport>,
) -> Vec<ProjectBumpRecommendation> {
    // Build lookup for brick deps by name
    let brick_deps: HashMap<&str, &[String]> = map
        .components
        .iter()
        .chain(map.bases.iter())
        .map(|b| (b.name.as_str(), b.deps.as_slice()))
        .collect();

    let mut recommendations = Vec::new();

    for project in &map.projects {
        // Get current project version from its Cargo.toml
        let cargo_toml_path = project.path.join("Cargo.toml");
        let current_version = std::fs::read_to_string(&cargo_toml_path)
            .ok()
            .and_then(|c| git::extract_version_from_cargo_toml_content(&c))
            .unwrap_or_else(|| "0.0.0".to_string());

        // Compute transitive closure of brick dependencies for this project
        let reachable_bricks = super::transitive_closure(
            project.deps.iter().cloned(),
            |name| {
                brick_deps
                    .get(name)
                    .map(|deps| deps.to_vec())
                    .unwrap_or_default()
            },
            |dep_key| vec![dep_key.to_owned()],
        );

        // Aggregate severity: worst-wins
        let mut worst_severity = ChangeSeverity::Unchanged;
        let mut changed_bricks = Vec::new();

        for brick_name in &reachable_bricks {
            if let Some(report) = brick_changes.get(brick_name) {
                if report.severity > worst_severity {
                    worst_severity = report.severity;
                }
                if report.severity > ChangeSeverity::Unchanged {
                    changed_bricks.push(brick_name.clone());
                }
            }
        }
        changed_bricks.sort();

        // Map severity to bump level
        let recommended_level = worst_severity.to_bump_level();

        let recommended_version = recommended_level
            .and_then(|level| compute_bumped_version(&current_version, level).ok())
            .map(|v| v.to_string());

        recommendations.push(ProjectBumpRecommendation {
            project_name: project.name.clone(),
            current_version,
            recommended_level,
            recommended_version,
            worst_severity,
            changed_bricks,
        });
    }

    recommendations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_severity_ordering() {
        assert!(ChangeSeverity::Unchanged < ChangeSeverity::TransitivePatch);
        assert!(ChangeSeverity::TransitivePatch < ChangeSeverity::InternalsChanged);
        assert!(ChangeSeverity::InternalsChanged < ChangeSeverity::InterfaceChanged);
    }

    #[test]
    fn change_severity_max_is_worst_wins() {
        let severities = vec![
            ChangeSeverity::Unchanged,
            ChangeSeverity::InternalsChanged,
            ChangeSeverity::TransitivePatch,
        ];
        let worst = severities.into_iter().max().unwrap();
        assert_eq!(worst, ChangeSeverity::InternalsChanged);
    }
}
