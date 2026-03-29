use std::collections::{HashMap, HashSet, VecDeque};

use super::app::{DepState, GridRow};

/// Pure grid state: rows, columns, cells, and the dependency graph used to
/// recompute transitive dependency states when direct deps are toggled.
pub struct Grid {
    pub rows: Vec<GridRow>,
    pub cells: Vec<Vec<DepState>>, // cells[row_idx][col_idx]
    pub modified_cols: Vec<bool>,
    pub modified_rows: Vec<bool>,
    /// Component dependency graph — used to recompute transitive states on toggle.
    pub comp_deps: HashMap<String, Vec<String>>,
    /// Direct deps per project column (indexed by col).
    pub project_direct_deps: Vec<Vec<String>>,
}

impl Grid {
    /// Toggle the cell at (row, col): Direct → None or None → Direct.
    /// Transitive cells are read-only.
    pub fn toggle(&mut self, row: usize, col: usize) {
        if row < self.rows.len() && col < self.n_cols() {
            match self.cells[row][col] {
                DepState::Direct => {
                    self.cells[row][col] = DepState::None;
                    self.modified_cols[col] = true;
                    self.recompute_transitive(col);
                }
                DepState::None => {
                    self.cells[row][col] = DepState::Direct;
                    self.modified_cols[col] = true;
                    self.recompute_transitive(col);
                }
                DepState::Transitive => {
                    // Transitive deps are read-only — toggle the parent instead.
                }
            }
        }
    }

    /// Recompute all Transitive/None states for the given column based on
    /// the current Direct selections.
    pub fn recompute_transitive(&mut self, col_i: usize) {
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

    /// Returns the upstream dependency chain if the cursor is on a Transitive cell.
    /// Chain is ordered from the direct-dep end to the target (hovered cell).
    /// E.g. `["cli", "mcp", "scaffold"]` means: project → cli → mcp → scaffold.
    pub fn chain_for_cursor(&self, cursor_row: usize, cursor_col: usize) -> Option<Vec<String>> {
        if self.cells.get(cursor_row)?.get(cursor_col).copied()? != DepState::Transitive {
            return None;
        }
        let target = &self.rows.get(cursor_row)?.name;
        let direct = self.project_direct_deps.get(cursor_col)?;
        find_chain(target, direct, &self.comp_deps)
    }

    /// Returns downstream BFS levels from the hovered cell.
    /// Each inner Vec contains the names of components at that BFS distance.
    /// Only includes components that appear as rows in the grid.
    /// Returns empty Vec if cursor is not on a Transitive cell.
    pub fn downstream_levels_for_cursor(
        &self,
        cursor_row: usize,
        cursor_col: usize,
    ) -> Vec<Vec<String>> {
        if self
            .cells
            .get(cursor_row)
            .and_then(|row| row.get(cursor_col))
            .copied()
            != Some(DepState::Transitive)
        {
            return vec![];
        }
        let hovered_name = match self.rows.get(cursor_row) {
            Some(row) => row.name.clone(),
            None => return vec![],
        };
        let row_names: HashSet<&str> = self.rows.iter().map(|r| r.name.as_str()).collect();
        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(hovered_name.clone());
        let mut frontier: Vec<String> = self
            .comp_deps
            .get(&hovered_name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|n| row_names.contains(n.as_str()) && !visited.contains(n))
            .collect();
        frontier.sort();
        while !frontier.is_empty() {
            for n in &frontier {
                visited.insert(n.clone());
            }
            let visible: Vec<String> = frontier
                .iter()
                .filter(|n| row_names.contains(n.as_str()))
                .cloned()
                .collect();
            if !visible.is_empty() {
                levels.push(visible);
            }
            let mut next: Vec<String> = frontier
                .iter()
                .flat_map(|n| self.comp_deps.get(n).cloned().unwrap_or_default())
                .filter(|n| !visited.contains(n) && row_names.contains(n.as_str()))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            next.sort();
            frontier = next;
        }
        levels
    }

    /// Number of columns (projects) in the grid.
    pub fn n_cols(&self) -> usize {
        if self.rows.is_empty() {
            self.modified_cols.len()
        } else {
            self.cells.first().map_or(0, |r| r.len())
        }
    }
}

/// BFS from `direct` through `comp_deps`, returning all transitively reachable
/// names that are NOT in `direct` itself.
pub(crate) fn compute_transitive(
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

/// BFS from `direct` deps, recording parent pointers, returning the shortest
/// path from a direct dep to `target`. Returns `None` if `target` is not reachable.
///
/// The returned `Vec` contains brick names from the first direct dep that reaches
/// `target` through to `target` itself, e.g.: `["cli", "mcp", "scaffold"]`.
fn find_chain(
    target: &str,
    direct: &[String],
    all_deps: &HashMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    let mut parent: HashMap<String, Option<String>> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for d in direct {
        if !parent.contains_key(d.as_str()) {
            parent.insert(d.clone(), None);
            queue.push_back(d.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        if current == target {
            let mut path = vec![current.clone()];
            let mut node = current.clone();
            while let Some(Some(p)) = parent.get(&node) {
                path.push(p.clone());
                node = p.clone();
            }
            path.reverse();
            return Some(path);
        }
        if let Some(deps) = all_deps.get(&current) {
            for dep in deps {
                if !parent.contains_key(dep.as_str()) {
                    parent.insert(dep.clone(), Some(current.clone()));
                    queue.push_back(dep.clone());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_deps(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|v| v.to_string()).collect()))
            .collect()
    }

    #[test]
    fn test_find_chain_simple() {
        let all_deps = make_deps(&[("a", &["b"]), ("b", &["c"])]);
        let direct = vec!["a".to_string()];
        let result = find_chain("c", &direct, &all_deps);
        assert_eq!(
            result,
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn test_find_chain_diamond() {
        let all_deps = make_deps(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"])]);
        let direct = vec!["a".to_string()];
        let result = find_chain("d", &direct, &all_deps);
        let chain = result.expect("should find a chain");
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0], "a");
        assert_eq!(*chain.last().unwrap(), "d");
    }

    #[test]
    fn test_find_chain_base_in_chain() {
        let all_deps = make_deps(&[("cli", &["mcp"]), ("mcp", &["scaffold"])]);
        let direct = vec!["cli".to_string()];
        let result = find_chain("scaffold", &direct, &all_deps);
        assert_eq!(
            result,
            Some(vec![
                "cli".to_string(),
                "mcp".to_string(),
                "scaffold".to_string()
            ])
        );
    }

    #[test]
    fn test_find_chain_not_reachable() {
        let all_deps = make_deps(&[("a", &["b"])]);
        let direct = vec!["a".to_string()];
        let result = find_chain("z", &direct, &all_deps);
        assert_eq!(result, None);
    }

    #[test]
    fn test_compute_transitive_excludes_direct() {
        let comp_deps = make_deps(&[("a", &["b", "c"]), ("b", &["c"])]);
        let direct: HashSet<String> = ["a".to_string()].into();
        let result = compute_transitive(&direct, &comp_deps);
        assert!(result.contains("b"));
        assert!(result.contains("c"));
        assert!(!result.contains("a")); // direct should be excluded
    }

    #[test]
    fn test_grid_toggle_direct_to_none() {
        use super::super::app::{GridRow, RowKind};
        use std::path::PathBuf;

        let rows = vec![GridRow {
            name: "comp-a".to_string(),
            kind: RowKind::Component,
            interface: None,
            path: PathBuf::from("components/comp-a"),
        }];
        let cells = vec![vec![DepState::Direct]];
        let mut grid = Grid {
            rows,
            cells,
            modified_cols: vec![false],
            modified_rows: vec![false],
            comp_deps: HashMap::new(),
            project_direct_deps: vec![vec!["comp-a".to_string()]],
        };

        grid.toggle(0, 0);
        assert_eq!(grid.cells[0][0], DepState::None);
        assert!(grid.modified_cols[0]);
    }

    #[test]
    fn test_grid_toggle_none_to_direct() {
        use super::super::app::{GridRow, RowKind};
        use std::path::PathBuf;

        let rows = vec![GridRow {
            name: "comp-a".to_string(),
            kind: RowKind::Component,
            interface: None,
            path: PathBuf::from("components/comp-a"),
        }];
        let cells = vec![vec![DepState::None]];
        let mut grid = Grid {
            rows,
            cells,
            modified_cols: vec![false],
            modified_rows: vec![false],
            comp_deps: HashMap::new(),
            project_direct_deps: vec![vec![]],
        };

        grid.toggle(0, 0);
        assert_eq!(grid.cells[0][0], DepState::Direct);
        assert!(grid.modified_cols[0]);
    }

    #[test]
    fn test_grid_toggle_transitive_is_noop() {
        use super::super::app::{GridRow, RowKind};
        use std::path::PathBuf;

        let rows = vec![GridRow {
            name: "comp-a".to_string(),
            kind: RowKind::Component,
            interface: None,
            path: PathBuf::from("components/comp-a"),
        }];
        let cells = vec![vec![DepState::Transitive]];
        let mut grid = Grid {
            rows,
            cells,
            modified_cols: vec![false],
            modified_rows: vec![false],
            comp_deps: HashMap::new(),
            project_direct_deps: vec![vec![]],
        };

        grid.toggle(0, 0);
        assert_eq!(grid.cells[0][0], DepState::Transitive);
        assert!(!grid.modified_cols[0]);
    }
}
