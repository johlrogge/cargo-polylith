use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_toml::Manifest;

use super::model::{Brick, BrickKind, Project, WorkspaceMap};

/// Walk up from `start` to find the workspace root Cargo.toml (the one with `[workspace]`).
pub fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join("Cargo.toml");
        if candidate.exists() {
            let manifest = Manifest::from_path(&candidate)
                .with_context(|| format!("failed to parse {}", candidate.display()))?;
            if manifest.workspace.is_some() {
                return Ok(current);
            }
        }
        if !current.pop() {
            anyhow::bail!(
                "no workspace Cargo.toml found starting from {}",
                start.display()
            );
        }
    }
}

/// Build a `WorkspaceMap` from the given workspace root.
pub fn build_workspace_map(root: &Path) -> Result<WorkspaceMap> {
    let components = scan_bricks(root, BrickKind::Component)?;
    let bases = scan_bricks(root, BrickKind::Base)?;
    let projects = scan_projects(root)?;
    Ok(WorkspaceMap {
        root: root.to_path_buf(),
        components,
        bases,
        projects,
    })
}

fn brick_dir(root: &Path, kind: &BrickKind) -> PathBuf {
    match kind {
        BrickKind::Component => root.join("components"),
        BrickKind::Base => root.join("bases"),
    }
}

fn scan_bricks(root: &Path, kind: BrickKind) -> Result<Vec<Brick>> {
    let dir = brick_dir(root, &kind);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut bricks = vec![];
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = Manifest::from_path(&manifest_path)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let name = manifest
            .package
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let deps = manifest
            .dependencies
            .keys()
            .cloned()
            .collect();
        bricks.push(Brick {
            name,
            kind: kind.clone(),
            path: path.clone(),
            deps,
            manifest_path,
        });
    }
    bricks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(bricks)
}

fn scan_projects(root: &Path) -> Result<Vec<Project>> {
    let dir = root.join("projects");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut projects = vec![];
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = Manifest::from_path(&manifest_path)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let name = manifest
            .package
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let members = manifest
            .workspace
            .as_ref()
            .map(|ws| {
                ws.members
                    .iter()
                    .map(|m| path.join(m))
                    .collect()
            })
            .unwrap_or_default();
        // [patch] tables are not directly exposed by cargo_toml; skip for now.
        projects.push(Project {
            name,
            path: path.clone(),
            members,
            patches: vec![],
        });
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}
