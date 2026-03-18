#![allow(dead_code)]

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
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub deps: Vec<String>,
    pub members: Vec<PathBuf>,
    pub patches: Vec<(String, PathBuf)>,
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
}
