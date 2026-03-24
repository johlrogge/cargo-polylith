use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

/// Shared package metadata from `[workspace.package]` in `Polylith.toml`.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspacePackageMeta {
    pub version: Option<String>,
    pub edition: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

/// Contents of `Polylith.toml` — the polylith workspace root marker.
#[derive(Debug, Clone, Serialize)]
pub struct PolylithToml {
    pub schema_version: u32,
    pub workspace_package: Option<WorkspacePackageMeta>,
    pub libraries: HashMap<String, ExternalDepInfo>,
    /// Maps profile name → relative path to `.profile` file.
    pub profiles: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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
    /// Dep keys that use a direct `path = "..."` dep (not `{ workspace = true }`).
    /// Used to detect bricks that bypass the workspace wiring diagram.
    pub path_dep_keys: Vec<String>,
}

/// Feature and version info for a single external (non-path) dependency.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalDepInfo {
    /// Sorted list of enabled features.
    pub features: Vec<String>,
    /// Version string, if present (None for git deps, path deps, etc.).
    pub version: Option<String>,
    /// Raw TOML dep value as written in [libraries] (e.g. `{ git = "...", rev = "..." }`).
    /// Used when `version` is None to emit the dep verbatim during workspace stripping.
    pub raw: Option<String>,
}

/// A path dependency declared in `[workspace.dependencies]` — the interface wiring diagram.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspacePathDep {
    /// Path value as written in the Cargo.toml (relative to workspace root).
    pub path: String,
    /// `package = "..."` alias, if present.
    pub package: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub deps: Vec<String>,
    /// True when this project's Cargo.toml contains its own `[workspace]` section.
    /// Projects must be plain bin crates in the root workspace, not sub-workspaces.
    pub has_own_workspace: bool,
    /// The `name` field from the first `[[bin]]` entry in the project's Cargo.toml, if any.
    #[allow(dead_code)] // populated during discovery; reserved for future use in output commands
    pub bin_name: Option<String>,
    /// Raw path dependencies: (dep_key, resolved_absolute_path). Used to validate
    /// that dep keys match the target package name. Only populated for deps that
    /// have a `path = "..."` value and no `package = "..."` alias.
    pub dep_paths: Vec<(String, PathBuf)>,
    /// External (non-path, non-workspace) deps with their features and version.
    /// Keyed by the resolved package name (or dep key when no `package =` alias).
    pub external_deps: HashMap<String, ExternalDepInfo>,
}

/// A polylith profile: a named set of implementation selections applied workspace-wide.
/// Profile files live at `profiles/<name>.profile` in the workspace root.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    /// Profile name derived from the filename (without `.profile` extension).
    pub name: String,
    /// Absolute path to the `.profile` file.
    pub path: PathBuf,
    /// Maps interface dep key → component path (relative to workspace root).
    pub implementations: HashMap<String, String>,
    /// Maps dep key → feature/version overrides for library deps.
    pub libraries: HashMap<String, ExternalDepInfo>,
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
    /// Path deps declared in root `[workspace.dependencies]` — the interface wiring diagram.
    /// Maps dep key (interface name) → path + optional package alias.
    pub root_workspace_interface_deps: HashMap<String, WorkspacePathDep>,
    /// Parsed `Polylith.toml` if present at root, `None` for legacy workspaces.
    pub polylith_toml: Option<PolylithToml>,
}

/// The fully resolved data needed to generate a profile workspace Cargo.toml.
/// Computed by `workspace::resolve_profile_workspace`; consumed by `scaffold::write_profile_workspace`.
#[derive(Debug, Clone)]
pub struct ResolvedProfileWorkspace {
    /// Profile name (used for the output path and header comment).
    pub profile_name: String,
    /// Workspace members as paths relative to the profile directory.
    pub members: Vec<String>,
    /// Interface (path) dep lines, fully rendered for [workspace.dependencies].
    /// Each entry is a TOML line like: `foo = { path = "../../components/foo" }`
    pub interface_dep_lines: Vec<String>,
    /// Library dep lines, fully rendered for [workspace.dependencies].
    pub library_dep_lines: Vec<String>,
    /// Shared package metadata from `Polylith.toml [workspace.package]`, if present.
    pub workspace_package: Option<WorkspacePackageMeta>,
}
