#![allow(dead_code)]

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum BrickKind {
    Component,
    Base,
}

#[derive(Debug, Clone)]
pub struct Brick {
    pub name: String,
    pub kind: BrickKind,
    pub path: PathBuf,
    pub deps: Vec<String>,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub members: Vec<PathBuf>,
    pub patches: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceMap {
    pub root: PathBuf,
    pub components: Vec<Brick>,
    pub bases: Vec<Brick>,
    pub projects: Vec<Project>,
}
