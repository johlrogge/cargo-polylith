use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::workspace::model::WorkspaceMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowKind {
    Component,
    Base,
}

#[derive(Debug, Clone)]
pub struct GridRow {
    pub name: String,
    pub kind: RowKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GridCol {
    pub name: String,
    pub path: PathBuf, // project directory
}

pub struct App {
    pub rows: Vec<GridRow>,
    pub cols: Vec<GridCol>,
    pub cells: Vec<Vec<bool>>, // cells[row_idx][col_idx]
    pub modified_cols: Vec<bool>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub status: String,
    pub quit: bool,
}

impl App {
    pub fn new(map: &WorkspaceMap) -> Result<Self> {
        let mut rows: Vec<GridRow> = map
            .components
            .iter()
            .map(|c| GridRow { name: c.name.clone(), kind: RowKind::Component, path: c.path.clone() })
            .collect();
        rows.extend(map.bases.iter().map(|b| GridRow {
            name: b.name.clone(),
            kind: RowKind::Base,
            path: b.path.clone(),
        }));

        let cols: Vec<GridCol> = map
            .projects
            .iter()
            .map(|p| GridCol { name: p.name.clone(), path: p.path.clone() })
            .collect();

        let cells: Vec<Vec<bool>> = rows
            .iter()
            .map(|row| {
                map.projects
                    .iter()
                    .map(|p| p.deps.iter().any(|d| d == &row.name))
                    .collect()
            })
            .collect();

        let modified_cols = vec![false; cols.len()];

        let status = if cols.is_empty() {
            "No projects — run: cargo polylith project new <name>".into()
        } else {
            "←→↑↓/hjkl: navigate  Space: toggle  w: write  q: quit".into()
        };

        Ok(App {
            rows,
            cols,
            cells,
            modified_cols,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            status,
            quit: false,
        })
    }

    pub fn n_components(&self) -> usize {
        self.rows.iter().filter(|r| r.kind == RowKind::Component).count()
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.rows.len() {
            self.cursor_row += 1;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_col + 1 < self.cols.len() {
            self.cursor_col += 1;
        }
    }

    /// Adjust scroll so cursor stays visible. Called from draw loop.
    pub fn scroll_to_cursor(&mut self, visible_rows: usize, visible_cols: usize) {
        let vr = visible_rows.max(1);
        let vc = visible_cols.max(1);

        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if self.cursor_row >= self.scroll_row + vr {
            self.scroll_row = self.cursor_row + 1 - vr;
        }

        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if self.cursor_col >= self.scroll_col + vc {
            self.scroll_col = self.cursor_col + 1 - vc;
        }
    }

    pub fn toggle_cell(&mut self) {
        let r = self.cursor_row;
        let c = self.cursor_col;
        if r < self.rows.len() && c < self.cols.len() {
            self.cells[r][c] = !self.cells[r][c];
            self.modified_cols[c] = true;
        }
    }

    pub fn write_all(&mut self) -> Result<()> {
        let mut written = 0usize;
        for col_i in 0..self.cols.len() {
            if !self.modified_cols[col_i] {
                continue;
            }
            let col_path = self.cols[col_i].path.clone();
            write_project_deps(&col_path, &self.rows, &self.cells, col_i)
                .with_context(|| format!("writing project '{}'", self.cols[col_i].name))?;
            self.modified_cols[col_i] = false;
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

/// Update `[dependencies]` in the project's Cargo.toml: add path deps for selected
/// rows, remove brick path deps for deselected rows, leave external deps untouched.
fn write_project_deps(
    project_path: &Path,
    rows: &[GridRow],
    cells: &[Vec<bool>],
    col_i: usize,
) -> Result<()> {
    let manifest_path = project_path.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let mut doc: DocumentMut = content.parse().context("parsing Cargo.toml")?;

    if doc.get("dependencies").is_none() {
        doc["dependencies"] = toml_edit::table();
    }
    let deps = doc["dependencies"].as_table_mut().context("[dependencies] is not a table")?;

    for (row_i, row) in rows.iter().enumerate() {
        let selected = cells.get(row_i).and_then(|r| r.get(col_i)).copied().unwrap_or(false);
        let dep_key = &row.name;

        if selected {
            // Add if not already present
            if deps.get(dep_key.as_str()).is_none() {
                let kind_dir = match row.kind {
                    RowKind::Component => "components",
                    RowKind::Base => "bases",
                };
                let path_str = format!("../../{}/{}", kind_dir, dep_key);
                let mut tbl = toml_edit::InlineTable::new();
                tbl.insert("path", toml_edit::Value::from(path_str));
                deps[dep_key.as_str()] =
                    toml_edit::Item::Value(toml_edit::Value::InlineTable(tbl));
            }
        } else {
            // Only remove if it's a brick path dep (not an external dep)
            if is_brick_dep(deps, dep_key) {
                deps.remove(dep_key.as_str());
            }
        }
    }

    fs::write(&manifest_path, doc.to_string())
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    Ok(())
}

/// Returns true if the dep entry has a `path` value pointing into components/ or bases/.
fn is_brick_dep(deps: &toml_edit::Table, name: &str) -> bool {
    let path_str = deps
        .get(name)
        .and_then(|item| {
            // InlineTable form: foo = { path = "..." }
            item.as_value()
                .and_then(|v| v.as_inline_table())
                .and_then(|t| t.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned())
                // Table form: [dependencies.foo] path = "..."
                .or_else(|| {
                    item.as_table()
                        .and_then(|t| t.get("path"))
                        .and_then(|v| v.as_value())
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned())
                })
        });
    path_str
        .as_deref()
        .map(|p| p.contains("/components/") || p.contains("/bases/"))
        .unwrap_or(false)
}
