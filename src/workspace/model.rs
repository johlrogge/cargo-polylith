#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrickKind {
    Component,
    Base,
}

#[derive(Debug, Clone, Serialize)]
pub struct Brick {
    pub name: String,
    pub kind: BrickKind,
    pub path: PathBuf,
    pub deps: Vec<String>,
    pub manifest_path: PathBuf,
    /// Value of `[package.metadata.polylith] interface = "..."`, if present.
    pub interface: Option<String>,
}

/// Feature and version info for a single external (non-path) dependency.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalDepInfo {
    /// Sorted list of enabled features.
    pub features: Vec<String>,
    /// Version string, if present (None for `{ workspace = true }` or version-less entries).
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub deps: Vec<String>,
    /// True when `[package.metadata.polylith] test-project = true` — suppresses `no-base` warning.
    pub test_project: bool,
    /// Raw path dependencies: (dep_key, resolved_absolute_path). Used to validate
    /// that dep keys match the target package name. Only populated for deps that
    /// have a `path = "..."` value and no `package = "..."` alias.
    pub dep_paths: Vec<(String, PathBuf)>,
    /// External (non-path, non-workspace) deps with their features and version.
    /// Keyed by the resolved package name (or dep key when no `package =` alias).
    pub external_deps: HashMap<String, ExternalDepInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceMap {
    pub root: PathBuf,
    pub components: Vec<Brick>,
    pub bases: Vec<Brick>,
    pub projects: Vec<Project>,
    /// Raw member patterns from the root `[workspace] members = [...]`.
    /// Empty if the root Cargo.toml has no members list.
    pub root_members: Vec<String>,
    /// False when the root Cargo.toml lacks a `[workspace]` section.
    /// Commands should warn the user in this case.
    pub is_workspace: bool,
    /// External (non-path) deps declared in root `[workspace.dependencies]`.
    /// Keyed by dep key (which equals the package name when no `package =` alias).
    pub root_workspace_deps: HashMap<String, ExternalDepInfo>,
}
