use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::workspace::model::{Brick, WorkspaceMap};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Projects,
    Bases,
}

/// One project plus the set of base names currently selected as its members.
#[derive(Debug)]
pub struct ProjectEntry {
    pub name: String,
    pub manifest_path: PathBuf,
    /// All known base names, with a bool indicating current membership.
    pub base_selections: Vec<(String, bool)>,
    pub modified: bool,
}

pub struct App {
    pub projects: Vec<ProjectEntry>,
    pub proj_idx: usize,
    pub base_idx: usize,
    pub focus: Focus,
    pub status: String,
    pub quit: bool,
}

impl App {
    pub fn new(map: &WorkspaceMap) -> Result<Self> {
        let all_bases: Vec<&Brick> = map.bases.iter().collect();

        let projects = map
            .projects
            .iter()
            .map(|p| {
                let manifest_path = p.path.join("Cargo.toml");
                let current_members = current_base_members(&manifest_path, &p.path);
                let base_selections = all_bases
                    .iter()
                    .map(|b| {
                        let selected = current_members.contains(&b.name);
                        (b.name.clone(), selected)
                    })
                    .collect();
                ProjectEntry {
                    name: p.name.clone(),
                    manifest_path,
                    base_selections,
                    modified: false,
                }
            })
            .collect();

        Ok(App {
            projects,
            proj_idx: 0,
            base_idx: 0,
            focus: Focus::Projects,
            status: String::from("Tab: switch pane  Space: toggle  w: write  q: quit"),
            quit: false,
        })
    }

    pub fn current_project(&self) -> Option<&ProjectEntry> {
        self.projects.get(self.proj_idx)
    }

    pub fn move_up(&mut self) {
        match self.focus {
            Focus::Projects => {
                if self.proj_idx > 0 {
                    self.proj_idx -= 1;
                    self.base_idx = 0;
                }
            }
            Focus::Bases => {
                if self.base_idx > 0 {
                    self.base_idx -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            Focus::Projects => {
                if self.proj_idx + 1 < self.projects.len() {
                    self.proj_idx += 1;
                    self.base_idx = 0;
                }
            }
            Focus::Bases => {
                let len = self
                    .projects
                    .get(self.proj_idx)
                    .map(|p| p.base_selections.len())
                    .unwrap_or(0);
                if self.base_idx + 1 < len {
                    self.base_idx += 1;
                }
            }
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Projects => Focus::Bases,
            Focus::Bases => Focus::Projects,
        };
    }

    pub fn toggle_base(&mut self) {
        if self.focus != Focus::Bases {
            return;
        }
        let idx = self.base_idx;
        if let Some(proj) = self.projects.get_mut(self.proj_idx) {
            if let Some(entry) = proj.base_selections.get_mut(idx) {
                entry.1 = !entry.1;
                proj.modified = true;
            }
        }
    }

    /// Write all modified project Cargo.toml files.
    pub fn write_all(&mut self) -> Result<()> {
        let mut written = 0usize;
        for proj in &mut self.projects {
            if !proj.modified {
                continue;
            }
            write_project_members(&proj.manifest_path, &proj.base_selections)
                .with_context(|| format!("writing {}", proj.manifest_path.display()))?;
            proj.modified = false;
            written += 1;
        }
        self.status = if written == 0 {
            "No changes to write.".into()
        } else {
            format!("Wrote {written} project(s).")
        };
        Ok(())
    }
}

/// Parse the project Cargo.toml and return the set of base names that are
/// currently listed as workspace members (matching `../../bases/<name>`).
fn current_base_members(manifest_path: &Path, project_dir: &Path) -> HashSet<String> {
    let content = match fs::read_to_string(manifest_path) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    let doc: DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(_) => return HashSet::new(),
    };
    let members = doc
        .get("workspace")
        .and_then(|ws| ws.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| project_dir.join(s))
                .filter_map(|p| {
                    // Normalise: strip the last component name and check the parent is "bases"
                    let name = p.file_name()?.to_string_lossy().into_owned();
                    let parent = p.parent()?;
                    if parent.file_name()?.to_string_lossy() == "bases" {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    members
}

/// Rewrite `[workspace].members` in the project Cargo.toml with the selected bases.
fn write_project_members(
    manifest_path: &Path,
    base_selections: &[(String, bool)],
) -> Result<()> {
    let content = fs::read_to_string(manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content
        .parse()
        .context("parsing project Cargo.toml")?;

    let workspace = doc
        .entry("workspace")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .context("'workspace' is not a table")?;

    let mut arr = toml_edit::Array::new();
    for (name, selected) in base_selections {
        if *selected {
            arr.push(format!("../../bases/{name}"));
        }
    }
    workspace["members"] = toml_edit::value(arr);

    fs::write(manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    Ok(())
}
