use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::workspace::model::WorkspaceMap;
use crate::scaffold::{BrickKind, DepEntry};
use super::grid::{Grid, compute_transitive};


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

/// Position of a cell within the hover chain visualization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChainPosition {
    /// Part of the upstream path from a direct dependency to the hovered cell.
    /// `step = 1` is the direct dependency entry point.
    Upstream { step: usize },
    /// Downstream from the hovered cell — a component it depends on.
    /// `level = 1` is a direct dependency of the hovered cell.
    /// Components at the same BFS level share the same number.
    Downstream { level: usize },
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
    pub grid: Grid,
    pub cols: Vec<GridCol>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub status: String,
    pub quit: bool,
    pub workspace_root: PathBuf,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub pending_g: bool,
    pub confirm_quit: bool,
    pub fold_active: bool,
    pub available_profiles: Vec<crate::workspace::Profile>,
    pub viewed_profile_idx: usize,
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

        let mut comp_deps: HashMap<String, Vec<String>> = map
            .components
            .iter()
            .map(|c| (c.name.clone(), c.deps.clone()))
            .collect();
        // Include base deps so transitive resolution works when projects depend on bases.
        for b in &map.bases {
            comp_deps.insert(b.name.clone(), b.deps.clone());
        }

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

        let project_direct_deps: Vec<Vec<String>> = map
            .projects
            .iter()
            .map(|p| p.deps.clone())
            .collect();

        let status = if cols.is_empty() {
            "Ctrl-Ctrl-n: new project  q: quit".into()
        } else {
            "←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  Ctrl-Ctrl-n: new project  q: quit".into()
        };

        let available_profiles = crate::workspace::discover_profiles(&map.root).unwrap_or_default();

        let grid = Grid {
            rows,
            cells,
            modified_cols,
            modified_rows,
            comp_deps,
            project_direct_deps,
        };

        Ok(App {
            grid,
            cols,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            status,
            quit: false,
            workspace_root: map.root.clone(),
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            pending_g: false,
            confirm_quit: false,
            fold_active: false,
            available_profiles,
            viewed_profile_idx: 0,
        })
    }

    pub fn n_components(&self) -> usize {
        self.grid.rows.iter().filter(|r| r.kind == RowKind::Component).count()
    }

    pub fn toggle_fold(&mut self) {
        let is_transitive = self
            .grid.cells
            .get(self.cursor_row)
            .and_then(|r| r.get(self.cursor_col))
            .copied()
            == Some(DepState::Transitive);
        if is_transitive {
            self.fold_active = !self.fold_active;
        }
    }

    fn current_cell_is_transitive(&self) -> bool {
        self.grid.cells
            .get(self.cursor_row)
            .and_then(|r| r.get(self.cursor_col))
            .copied()
            == Some(DepState::Transitive)
    }

    pub fn move_up(&mut self) {
        if self.fold_active {
            let mut chain_names: HashSet<String> = self
                .chain_for_cursor()
                .unwrap_or_default()
                .into_iter()
                .collect();
            for level in self.downstream_levels_for_cursor() {
                chain_names.extend(level);
            }
            let mut r = self.cursor_row;
            loop {
                if r == 0 {
                    break;
                }
                r -= 1;
                if chain_names.contains(&self.grid.rows[r].name) {
                    self.cursor_row = r;
                    break;
                }
            }
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
        if !self.current_cell_is_transitive() {
            self.fold_active = false;
        }
    }

    pub fn move_down(&mut self) {
        if self.fold_active {
            let mut chain_names: HashSet<String> = self
                .chain_for_cursor()
                .unwrap_or_default()
                .into_iter()
                .collect();
            for level in self.downstream_levels_for_cursor() {
                chain_names.extend(level);
            }
            let mut r = self.cursor_row;
            loop {
                if r + 1 >= self.grid.rows.len() {
                    break;
                }
                r += 1;
                if chain_names.contains(&self.grid.rows[r].name) {
                    self.cursor_row = r;
                    break;
                }
            }
        } else if self.cursor_row + 1 < self.grid.rows.len() {
            self.cursor_row += 1;
        }
        if !self.current_cell_is_transitive() {
            self.fold_active = false;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
        if !self.current_cell_is_transitive() {
            self.fold_active = false;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_col + 1 < self.cols.len() {
            self.cursor_col += 1;
        }
        if !self.current_cell_is_transitive() {
            self.fold_active = false;
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
        self.grid.toggle(self.cursor_row, self.cursor_col);
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
        crate::commands::validate::validate_brick_name(&name)?;

        crate::scaffold::create_project(&self.workspace_root, &name)
            .with_context(|| format!("creating project '{name}'"))?;

        let project_dir = self.workspace_root.join("projects").join(&name);
        let new_col = GridCol { name: name.clone(), path: project_dir };
        for row_cells in &mut self.grid.cells {
            row_cells.push(DepState::None);
        }
        self.grid.modified_cols.push(false);
        let new_col_idx = self.cols.len();
        self.cols.push(new_col);
        self.cursor_col = new_col_idx;
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status = format!("Created project '{name}'.  ←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  Ctrl-n: new project  q: quit");
        Ok(())
    }

    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status = if self.cols.is_empty() {
            "Ctrl-n: new project  q: quit".into()
        } else {
            "←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  Ctrl-n: new project  q: quit".into()
        };
    }

    pub fn start_edit_interface(&mut self) {
        let row = &self.grid.rows[self.cursor_row];
        if row.kind != RowKind::Component {
            self.status = "Bases do not have interfaces".into();
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
        self.grid.rows[row_i].interface = Some(iface);
        self.grid.modified_rows[row_i] = true;
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status = "Interface staged — press w to write.  ←→↑↓/hjkl: navigate  Space: toggle  i: interface  w: write  Ctrl-n: new project  q: quit".into();
    }

    /// Returns the raw dependency chain if the cursor is on a Transitive cell.
    /// Chain is ordered from the direct-dep end to the target (hovered cell).
    /// E.g. ["cli", "mcp", "scaffold"] means: project → cli → mcp → scaffold.
    pub fn chain_for_cursor(&self) -> Option<Vec<String>> {
        self.grid.chain_for_cursor(self.cursor_row, self.cursor_col)
    }

    /// Returns downstream BFS levels from the hovered cell.
    /// Each inner Vec contains the names of components at that BFS distance from the cursor.
    /// Only includes components that actually appear as rows in the grid.
    /// Returns empty Vec if cursor is not on a Transitive cell.
    pub fn downstream_levels_for_cursor(&self) -> Vec<Vec<String>> {
        self.grid.downstream_levels_for_cursor(self.cursor_row, self.cursor_col)
    }

    pub fn write_all(&mut self) -> Result<()> {
        let mut written = 0usize;
        for col_i in 0..self.cols.len() {
            if !self.grid.modified_cols[col_i] {
                continue;
            }
            let col_path = self.cols[col_i].path.clone();
            let entries: Vec<DepEntry> = self.grid.rows.iter().enumerate().map(|(row_i, row)| {
                let selected = self.grid.cells
                    .get(row_i)
                    .and_then(|r| r.get(col_i))
                    .map(|&s| s == DepState::Direct)
                    .unwrap_or(false);
                DepEntry {
                    name: row.name.clone(),
                    interface: row.interface.clone(),
                    kind: match row.kind {
                        RowKind::Component => BrickKind::Component,
                        RowKind::Base => BrickKind::Base,
                    },
                    path: row.path.clone(),
                    selected,
                }
            }).collect();
            crate::scaffold::write_project_deps(&col_path, &entries)
                .with_context(|| format!("writing project '{}'", self.cols[col_i].name))?;
            self.grid.modified_cols[col_i] = false;
            written += 1;
        }
        for row_i in 0..self.grid.rows.len() {
            if !self.grid.modified_rows[row_i] {
                continue;
            }
            if let Some(iface) = self.grid.rows[row_i].interface.clone() {
                let row_path = self.grid.rows[row_i].path.clone();
                let row_name = self.grid.rows[row_i].name.clone();
                crate::scaffold::write_interface_to_toml(&row_path, &iface)
                    .with_context(|| format!("writing interface for '{row_name}'"))?;
                self.grid.modified_rows[row_i] = false;
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

    /// Returns true if this row belongs to an interface implemented by 2+ components.
    /// Only multi-impl interface rows use radio button rendering and profile toggling.
    pub fn is_multi_impl_interface(&self, row_i: usize) -> bool {
        match self.grid.rows.get(row_i).and_then(|r| r.interface.as_deref()) {
            Some(iface) => self.grid.rows.iter()
                .filter(|r| r.interface.as_deref() == Some(iface))
                .count() >= 2,
            None => false,
        }
    }

    pub fn toggle_profile_impl(&mut self, row_i: usize) -> anyhow::Result<()> {
        if self.available_profiles.is_empty() {
            return Ok(());
        }
        let profile_idx = self.viewed_profile_idx;
        let iface = match self.grid.rows.get(row_i).and_then(|r| r.interface.clone()) {
            Some(i) => i,
            None => return Ok(()),
        };
        let rel_path = self.grid.rows[row_i].path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(&self.grid.rows[row_i].path)
            .to_string_lossy()
            .into_owned();
        // No-op if this implementation is already selected
        let current = self.available_profiles[profile_idx].implementations.get(&iface).cloned();
        if current.as_deref() == Some(rel_path.as_str()) {
            return Ok(());
        }
        self.available_profiles[profile_idx].implementations.insert(iface.clone(), rel_path.clone());
        let profile_path = self.available_profiles[profile_idx].path.clone();
        crate::scaffold::write_profile_impl(&profile_path, &iface, &rel_path)?;
        Ok(())
    }
}
