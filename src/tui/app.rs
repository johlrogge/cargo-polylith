use std::collections::{HashMap, HashSet, VecDeque};
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

/// Whether a row (component/base) is a direct, transitive, or absent dependency
/// of a given project column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepState {
    /// Not a dependency.
    None,
    /// Pulled in transitively through another component — read-only in the editor.
    Transitive,
    /// Listed directly in the project's `[dependencies]`.
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    CreatingProject,
    EditingInterface,
}

#[derive(Debug, Clone)]
pub struct GridRow {
    pub name: String,
    pub kind: RowKind,
    pub interface: Option<String>,
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
    pub cells: Vec<Vec<DepState>>, // cells[row_idx][col_idx]
    pub modified_cols: Vec<bool>,
    pub modified_rows: Vec<bool>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub status: String,
    pub quit: bool,
    pub workspace_root: PathBuf,
    pub input_mode: InputMode,
    pub input_buffer: String,
    /// Component dependency graph — used to recompute transitive states on toggle.
    comp_deps: HashMap<String, Vec<String>>,
}

impl App {
    pub fn new(map: &WorkspaceMap) -> Result<Self> {
        // Sort components: interface groups first (alphabetically), no-interface last,
        // within each group by name.
        let mut comp_rows: Vec<GridRow> = map
            .components
            .iter()
            .map(|c| GridRow {
                name: c.name.clone(),
                kind: RowKind::Component,
                interface: c.interface.clone(),
                path: c.path.clone(),
            })
            .collect();
        comp_rows.sort_by(|a, b| match (&a.interface, &b.interface) {
            (Some(ai), Some(bi)) => ai.cmp(bi).then(a.name.cmp(&b.name)),
            (Some(_), None)      => std::cmp::Ordering::Less,
            (None, Some(_))      => std::cmp::Ordering::Greater,
            (None, None)         => a.name.cmp(&b.name),
        });
        let mut rows = comp_rows;
        rows.extend(map.bases.iter().map(|b| GridRow {
            name: b.name.clone(),
            kind: RowKind::Base,
            interface: None,
            path: b.path.clone(),
        }));

        let cols: Vec<GridCol> = map
            .projects
            .iter()
            .map(|p| GridCol { name: p.name.clone(), path: p.path.clone() })
            .collect();

        let comp_deps: HashMap<String, Vec<String>> = map
            .components
            .iter()
            .map(|c| (c.name.clone(), c.deps.clone()))
            .collect();

        // Build cells: compute direct and transitive deps per project column.
        let n_rows = rows.len();
        let n_cols = cols.len();
        let mut cells = vec![vec![DepState::None; n_cols]; n_rows];
        for (col_i, project) in map.projects.iter().enumerate() {
            let direct: HashSet<String> = project.deps.iter().cloned().collect();
            let transitive = compute_transitive(&direct, &comp_deps);
            for (row_i, row) in rows.iter().enumerate() {
                cells[row_i][col_i] = if direct.contains(&row.name) {
                    DepState::Direct
                } else if transitive.contains(&row.name) {
                    DepState::Transitive
                } else {
                    DepState::None
                };
            }
        }

        let modified_cols = vec![false; n_cols];
        let modified_rows = vec![false; n_rows];

        let status = if cols.is_empty() {
            "n: new project  q: quit".into()
        } else {
            "←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  n: new project  q: quit".into()
        };

        Ok(App {
            rows,
            cols,
            cells,
            modified_cols,
            modified_rows,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            status,
            quit: false,
            workspace_root: map.root.clone(),
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            comp_deps,
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
            match self.cells[r][c] {
                DepState::Direct => {
                    self.cells[r][c] = DepState::None;
                    self.modified_cols[c] = true;
                    self.recompute_transitive(c);
                }
                DepState::None => {
                    self.cells[r][c] = DepState::Direct;
                    self.modified_cols[c] = true;
                    self.recompute_transitive(c);
                }
                DepState::Transitive => {
                    // Transitive deps are read-only — toggle the parent instead.
                }
            }
        }
    }

    fn recompute_transitive(&mut self, col_i: usize) {
        let direct: HashSet<String> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(row_i, _)| self.cells[*row_i][col_i] == DepState::Direct)
            .map(|(_, row)| row.name.clone())
            .collect();
        let transitive = compute_transitive(&direct, &self.comp_deps);
        for (row_i, row) in self.rows.iter().enumerate() {
            if self.cells[row_i][col_i] != DepState::Direct {
                self.cells[row_i][col_i] = if transitive.contains(&row.name) {
                    DepState::Transitive
                } else {
                    DepState::None
                };
            }
        }
    }

    pub fn start_create_project(&mut self) {
        self.input_mode = InputMode::CreatingProject;
        self.input_buffer.clear();
        self.status = "New project name: ".into();
    }

    pub fn input_char(&mut self, ch: char) {
        self.input_buffer.push(ch);
    }

    pub fn input_backspace(&mut self) {
        self.input_buffer.pop();
    }

    pub fn confirm_create_project(&mut self) -> Result<()> {
        let name = self.input_buffer.trim().to_owned();
        anyhow::ensure!(!name.is_empty(), "project name cannot be empty");
        anyhow::ensure!(
            name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
            "project name must contain only alphanumeric characters, hyphens, or underscores"
        );

        let project_dir = self.workspace_root.join("projects").join(&name);
        fs::create_dir_all(project_dir.join("src"))
            .with_context(|| format!("creating {}", project_dir.display()))?;

        let cargo_toml = format!(
            "[workspace]\nresolver = \"2\"\nmembers = []\n\n[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"{name}\"\npath = \"src/main.rs\"\n\n[dependencies]\n"
        );
        fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
            .with_context(|| format!("writing Cargo.toml for {name}"))?;
        fs::write(project_dir.join("src/main.rs"), "fn main() {}\n")
            .with_context(|| format!("writing src/main.rs for {name}"))?;

        let new_col = GridCol { name: name.clone(), path: project_dir };
        for row_cells in &mut self.cells {
            row_cells.push(DepState::None);
        }
        self.modified_cols.push(false);
        let new_col_idx = self.cols.len();
        self.cols.push(new_col);
        self.cursor_col = new_col_idx;
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status = format!("Created project '{name}'.  ←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  n: new project  q: quit");
        Ok(())
    }

    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status = if self.cols.is_empty() {
            "n: new project  q: quit".into()
        } else {
            "←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  n: new project  q: quit".into()
        };
    }

    pub fn start_edit_interface(&mut self) {
        let row = &self.rows[self.cursor_row];
        if row.kind != RowKind::Component {
            return;
        }
        self.input_mode = InputMode::EditingInterface;
        self.input_buffer = row.interface.clone().unwrap_or_else(|| row.name.clone());
    }

    pub fn confirm_edit_interface(&mut self) {
        let iface = self.input_buffer.trim().to_owned();
        if iface.is_empty() {
            self.cancel_input();
            return;
        }
        let row_i = self.cursor_row;
        self.rows[row_i].interface = Some(iface);
        self.modified_rows[row_i] = true;
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status = "Interface staged — press w to write.  ←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  n: new project  q: quit".into();
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
        for row_i in 0..self.rows.len() {
            if !self.modified_rows[row_i] {
                continue;
            }
            if let Some(iface) = self.rows[row_i].interface.clone() {
                let row_path = self.rows[row_i].path.clone();
                let row_name = self.rows[row_i].name.clone();
                crate::scaffold::write_interface_to_toml(&row_path, &iface)
                    .with_context(|| format!("writing interface for '{row_name}'"))?;
                self.modified_rows[row_i] = false;
                written += 1;
            }
        }
        self.status = if written == 0 {
            "No changes to write.".into()
        } else {
            format!("Wrote {written} change(s).")
        };
        Ok(())
    }
}

/// Update `[dependencies]` in the project's Cargo.toml: add path deps for direct-dep
/// rows, remove brick path deps for deselected rows, leave external deps untouched.
fn write_project_deps(
    project_path: &Path,
    rows: &[GridRow],
    cells: &[Vec<DepState>],
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
        let selected = cells
            .get(row_i)
            .and_then(|r| r.get(col_i))
            .map(|&s| s == DepState::Direct)
            .unwrap_or(false);
        // Use the polylith interface name as the dep key when it differs from the
        // crate name — this enables substitution (e.g. stub vs real) without
        // changing call-site code. Cargo's `package` key handles the rename.
        let dep_key = row.interface.as_deref()
            .filter(|iface| *iface != row.name.as_str())
            .unwrap_or(&row.name);

        if selected {
            // Add if not already present
            if deps.get(dep_key).is_none() {
                let kind_dir = match row.kind {
                    RowKind::Component => "components",
                    RowKind::Base => "bases",
                };
                let dir_name = row.path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let path_str = format!("../../{}/{}", kind_dir, dir_name);
                let mut tbl = toml_edit::InlineTable::new();
                tbl.insert("path", toml_edit::Value::from(path_str));
                // If the crate name differs from the interface/dep key, add `package`
                // so Cargo knows which crate to actually pull in.
                if dep_key != row.name.as_str() {
                    tbl.insert("package", toml_edit::Value::from(row.name.clone()));
                }
                deps[dep_key] =
                    toml_edit::Item::Value(toml_edit::Value::InlineTable(tbl));
            }
        } else {
            // Only remove if it's a brick path dep (not an external dep)
            if is_brick_dep(deps, dep_key) {
                deps.remove(dep_key);
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

/// BFS from `direct` through `comp_deps`, returning all transitively reachable
/// names that are NOT in `direct` itself.
fn compute_transitive(
    direct: &HashSet<String>,
    comp_deps: &HashMap<String, Vec<String>>,
) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = direct.iter().cloned().collect();
    while let Some(name) = queue.pop_front() {
        if reachable.insert(name.clone()) {
            if let Some(deps) = comp_deps.get(&name) {
                for d in deps {
                    if !reachable.contains(d) {
                        queue.push_back(d.clone());
                    }
                }
            }
        }
    }
    reachable.retain(|n| !direct.contains(n));
    reachable
}
