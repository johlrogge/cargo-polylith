use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

use crate::corsett::FoldEntry;
use super::app::{App, ChainPosition, DepState, InputMode, RowKind};

const IFACE_WIDTH: u16 = 16; // interface label column (left)
const IMPL_WIDTH: u16 = 22;  // component/base name column
const LABEL_WIDTH: u16 = IFACE_WIDTH + IMPL_WIDTH; // total label area
const COL_WIDTH: u16 = 2; // cell char + space

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // profile tabs
            Constraint::Min(3),     // grid
            Constraint::Length(1),  // status bar
        ])
        .split(area);

    draw_profile_tabs(frame, app, layout[0]);
    draw_grid(frame, app, layout[1]);
    draw_status(frame, app, layout[2]);
}

fn draw_profile_tabs(frame: &mut Frame, app: &App, area: Rect) {
    if app.available_profiles.is_empty() {
        return;
    }
    let buf = frame.buffer_mut();
    let mut x = area.x + 1;
    for (i, profile) in app.available_profiles.iter().enumerate() {
        let label = format!("[{}: {}]", i + 1, profile.name);
        let style = if i == app.viewed_profile_idx {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        if x < area.x + area.width {
            buf.set_string(x, area.y, &label, style);
            x += label.chars().count() as u16 + 1;
        }
    }
}

fn draw_grid(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().title(" cargo polylith ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.cols.is_empty() && app.rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No projects, components, or bases found.\nPress 'n' to create a new project."),
            inner,
        );
        return;
    }

    if app.cols.is_empty() {
        frame.render_widget(
            Paragraph::new("No projects found.\nPress 'n' to create a new project."),
            inner,
        );
        return;
    }

    if app.rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No components or bases found."),
            inner,
        );
        return;
    }

    let n_components = app.n_components();
    let n_bases = app.rows.len() - n_components;

    // Section header rows: 1 per non-empty section
    let section_rows: u16 = (if n_components > 0 { 1 } else { 0 })
        + (if n_bases > 0 { 1 } else { 0 });

    // Header shrinks as the user scrolls down — one row of budget removed per
    // row scrolled — but never below the minimum height needed to keep all
    // project names unique (the corsett floor).
    let col_names: Vec<&str> = app.cols.iter().map(|c| c.name.as_str()).collect();
    let full_header_h = col_names.iter().map(|n| n.chars().count()).max().unwrap_or(1);
    let min_unique_h = crate::corsett::min_group_height(&col_names);
    let target_h = full_header_h
        .saturating_sub(app.scroll_row)
        .max(min_unique_h);

    let col_display_names = crate::corsett::fit_group(&col_names, target_h);

    // Actual header height = longest display name (may be less than target_h).
    let header_rows = col_display_names
        .iter()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(1) as u16;

    let data_area_h = inner.height.saturating_sub(header_rows + section_rows);
    let grid_w = inner.width.saturating_sub(LABEL_WIDTH);
    let visible_cols = (grid_w / COL_WIDTH) as usize;
    let visible_rows = data_area_h as usize;

    app.scroll_to_cursor(visible_rows.max(1), visible_cols.max(1));

    let scroll_row = app.scroll_row;
    let scroll_col = app.scroll_col;

    // ── Column headers (project names, written top-to-bottom) ──────────────
    let vis_cols = visible_cols.min(app.cols.len().saturating_sub(scroll_col));
    for sc in 0..vis_cols {
        let col_i = scroll_col + sc;
        let name = &col_display_names[col_i];
        let x = inner.x + LABEL_WIDTH + sc as u16 * COL_WIDTH;
        let is_cursor_col = col_i == app.cursor_col;
        let style = if is_cursor_col {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        for hr in 0..header_rows {
            let y = inner.y + hr;
            if y >= inner.y + inner.height {
                break;
            }
            let ch = name.chars().nth(hr as usize).unwrap_or(' ').to_string();
            let buf = frame.buffer_mut();
            if x < inner.x + inner.width {
                buf.set_string(x, y, &ch, style);
            }
        }
    }

    // Build combined chain map: name → ChainPosition
    let chain_upstream: Vec<String> = app.chain_for_cursor().unwrap_or_default();
    let chain_down_levels: Vec<Vec<String>> = app.downstream_levels_for_cursor();

    let mut chain_map: std::collections::HashMap<String, ChainPosition> = std::collections::HashMap::new();
    for (i, name) in chain_upstream.iter().enumerate() {
        chain_map.insert(name.clone(), ChainPosition::Upstream { step: i + 1 });
    }
    for (level_idx, level) in chain_down_levels.iter().enumerate() {
        for name in level {
            chain_map.entry(name.clone()).or_insert(ChainPosition::Downstream { level: level_idx + 1 });
        }
    }

    // Interfaces with 2+ implementations → radio-button rendering
    let multi_impl_interfaces: std::collections::HashSet<&str> = {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for row in &app.rows {
            if let Some(iface) = row.interface.as_deref() {
                *counts.entry(iface).or_insert(0) += 1;
            }
        }
        counts.into_iter().filter(|&(_, n)| n >= 2).map(|(k, _)| k).collect()
    };

    // Radio buttons only meaningful when profiles are loaded
    let has_profiles = !app.available_profiles.is_empty();

    // Pre-compute viewed profile's impl map
    let viewed_impl_map: std::collections::HashMap<&str, &str> = if has_profiles {
        app.available_profiles
            .get(app.viewed_profile_idx)
            .map(|p| p.implementations.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect())
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    // ── Data rows ──────────────────────────────────────────────────────────
    let mut display_y = inner.y + header_rows;
    let bottom = inner.y + inner.height;

    // Build fold plan: binary — chain rows shown, everything else hidden
    let cursor_row_name = app.rows.get(app.cursor_row).map(|r| r.name.as_str()).unwrap_or("");

    let fold_plan: Vec<FoldEntry> = if app.fold_active && !chain_map.is_empty() {
        // Sort chain rows in dependency order:
        // 1. Upstream rows (step order: direct dep first)
        // 2. Cursor row
        // 3. Downstream rows (level order)
        let mut upstream_by_step: Vec<(usize, usize)> = Vec::new(); // (step, row_idx)
        let mut downstream_by_level: Vec<(usize, usize)> = Vec::new(); // (level, row_idx)
        for (row_i, row) in app.rows.iter().enumerate() {
            if row.name == cursor_row_name { continue; }
            match chain_map.get(&row.name).copied() {
                Some(ChainPosition::Upstream { step }) => {
                    upstream_by_step.push((step, row_i));
                }
                Some(ChainPosition::Downstream { level }) => {
                    downstream_by_level.push((level, row_i));
                }
                None => {}
            }
        }
        upstream_by_step.sort_by_key(|&(step, _)| step);
        downstream_by_level.sort_by_key(|&(level, _)| level);

        let mut ordered_chain: Vec<usize> = Vec::new();
        for (_, row_i) in upstream_by_step { ordered_chain.push(row_i); }
        ordered_chain.push(app.cursor_row);
        for (_, row_i) in downstream_by_level { ordered_chain.push(row_i); }

        let chain_set: std::collections::HashSet<usize> = ordered_chain.iter().copied().collect();
        let non_chain_count = app.rows.len().saturating_sub(chain_set.len());

        let mut plan: Vec<FoldEntry> = Vec::new();
        if non_chain_count > 0 {
            plan.push(FoldEntry::Hidden(non_chain_count));
        }
        for row_i in ordered_chain {
            plan.push(FoldEntry::Row(row_i));
        }
        plan
    } else {
        (scroll_row..app.rows.len())
            .map(FoldEntry::Row)
            .collect()
    };

    // Track section headers emitted to avoid duplicates
    let mut components_header_shown = false;
    let mut bases_header_shown = false;

    for entry in fold_plan {
        if display_y >= bottom {
            break;
        }

        let row_i = match entry {
            FoldEntry::Hidden(count) => {
                // Render a single dimmed placeholder row
                let text = format!("  \u{27e8}{count} rows hidden\u{27e9}");
                let buf = frame.buffer_mut();
                buf.set_string(
                    inner.x,
                    display_y,
                    &text,
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                );
                display_y += 1;
                continue;
            }
            FoldEntry::Row(i) => i,
        };
        let row_kind = app.rows[row_i].kind;

        // Section header before first component
        if row_i < n_components && n_components > 0 && !components_header_shown && display_y < bottom {
            draw_section_header(frame, inner, display_y, "── components");
            display_y += 1;
            components_header_shown = true;
        }

        // Section header before first base
        if row_i >= n_components && n_bases > 0 && !bases_header_shown && display_y < bottom {
            draw_section_header(frame, inner, display_y, "── bases");
            display_y += 1;
            bases_header_shown = true;
        }

        if display_y >= bottom {
            break;
        }

        let is_cursor_row = row_i == app.cursor_row;

        // Interface column — show only on the first row of each interface group.
        let is_iface_start = match app.rows[row_i].interface.as_deref() {
            None => false,
            Some(iface) => row_i == 0 || app.rows[row_i - 1].interface.as_deref() != Some(iface),
        };
        let iface_str = if is_iface_start {
            app.rows[row_i].interface.as_deref().unwrap_or("")
        } else {
            ""
        };
        {
            let label = truncate(iface_str, (IFACE_WIDTH - 1) as usize);
            let padded = format!("{:<width$}", label, width = (IFACE_WIDTH - 1) as usize);
            let buf = frame.buffer_mut();
            buf.set_string(inner.x, display_y, &padded, Style::default().fg(Color::DarkGray));
        }

        // Impl / base name column
        let label = truncate(&app.rows[row_i].name, (IMPL_WIDTH - 1) as usize);
        let padded = format!("{:<width$}", label, width = (IMPL_WIDTH - 1) as usize);
        let label_style = match (row_kind, is_cursor_row) {
            (RowKind::Component, true) => {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            }
            (RowKind::Component, false) => Style::default().fg(Color::Green),
            (RowKind::Base, true) => {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            }
            (RowKind::Base, false) => Style::default().fg(Color::Cyan),
        };
        {
            let buf = frame.buffer_mut();
            buf.set_string(inner.x + IFACE_WIDTH, display_y, &padded, label_style);
        }

        // Cells
        for sc in 0..vis_cols {
            let col_i = scroll_col + sc;
            let dep_state = app
                .cells
                .get(row_i)
                .and_then(|r| r.get(col_i))
                .copied()
                .unwrap_or(DepState::None);
            let modified = app.modified_cols.get(col_i).copied().unwrap_or(false);
            let is_cursor = is_cursor_row && col_i == app.cursor_col;

            // Chain highlighting: only in the cursor column, not on the cursor cell itself
            let chain_mark: Option<ChainPosition> = if col_i == app.cursor_col && !is_cursor {
                let row_name = app.rows.get(row_i).map(|r| r.name.as_str()).unwrap_or("");
                chain_map.get(row_name).copied()
            } else {
                None
            };

            // Radio-button rendering for multi-implementation interface groups.
            // Computed before the cursor/chain branches so cursor cells can also show ◉/○.
            let is_radio = app.rows.get(row_i)
                .and_then(|r| r.interface.as_deref())
                .map(|iface| multi_impl_interfaces.contains(iface))
                .unwrap_or(false);

            let radio_selected = app.rows.get(row_i)
                .and_then(|r| r.interface.as_deref())
                .and_then(|iface| viewed_impl_map.get(iface))
                .map(|&sel| {
                    let rel = app.rows[row_i].path
                        .strip_prefix(&app.workspace_root).ok()
                        .map(|p| p.to_string_lossy().into_owned());
                    rel.as_deref() == Some(sel)
                })
                .unwrap_or(false);

            // Radio only shown when this project column actually has some relationship with
            // this interface (at least one implementation is Direct or Transitive for this column)
            let project_uses_interface = if is_radio {
                let iface = app.rows.get(row_i).and_then(|r| r.interface.as_deref());
                iface.map(|iface_name| {
                    app.rows.iter().enumerate().any(|(ri, r)| {
                        r.interface.as_deref() == Some(iface_name)
                        && app.cells.get(ri).and_then(|row| row.get(col_i))
                            .copied().unwrap_or(DepState::None) != DepState::None
                    })
                }).unwrap_or(false)
            } else {
                false
            };
            let effective_is_radio = is_radio && project_uses_interface;

            let (ch, style) = if let Some(pos) = chain_mark {
                match pos {
                    ChainPosition::Upstream { step: 1 } => {
                        // Direct-dep entry point: light green bullet
                        (
                            "\u{25cf}".to_string(), // ●
                            Style::default()
                                .fg(Color::Rgb(150, 255, 150))
                                .add_modifier(Modifier::BOLD),
                        )
                    }
                    ChainPosition::Upstream { step } => {
                        // Upstream transitive hops: blue numbered (display = step - 1)
                        let display_step = step - 1;
                        let digit = if display_step <= 9 {
                            char::from_digit(display_step as u32, 10).unwrap_or('+')
                        } else {
                            '+'
                        };
                        // Slight fade with each step
                        let blue = 255u8.saturating_sub(((display_step.saturating_sub(1)) * 20) as u8);
                        let color = Color::Rgb(80, 150, blue);
                        (
                            digit.to_string(),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        )
                    }
                    ChainPosition::Downstream { level } => {
                        // Downstream: same level number shared across parallel deps, warmer blue-cyan
                        let digit = if level <= 9 {
                            char::from_digit(level as u32, 10).unwrap_or('+')
                        } else {
                            '+'
                        };
                        let intensity = 220u8.saturating_sub((level.saturating_sub(1) * 30) as u8);
                        (
                            digit.to_string(),
                            Style::default()
                                .fg(Color::Rgb(80, intensity, 200))
                                .add_modifier(Modifier::BOLD),
                        )
                    }
                }
            } else if is_cursor {
                let ch = if effective_is_radio {
                    if radio_selected { "\u{25c9}".to_string() }
                    else              { "\u{25cb}".to_string() }
                } else {
                    match dep_state {
                        DepState::Direct    => "x".to_string(),
                        DepState::Transitive => "·".to_string(),
                        DepState::None      => "-".to_string(),
                    }
                };
                (ch, Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD))
            } else {
                match (radio_selected, effective_is_radio) {
                    (true, true) => (
                        "\u{25c9}".to_string(), // ◉
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    ),
                    (false, true) => (
                        "\u{25cb}".to_string(), // ○
                        Style::default().fg(Color::DarkGray),
                    ),
                    _ => (
                        match dep_state {
                            DepState::Direct => "x".to_string(),
                            DepState::Transitive => "·".to_string(),
                            DepState::None => "-".to_string(),
                        },
                        match dep_state {
                            DepState::Direct => {
                                let base = Style::default().fg(Color::Green);
                                if modified { base.add_modifier(Modifier::BOLD) } else { base }
                            }
                            DepState::Transitive => Style::default().fg(Color::Gray),
                            DepState::None => Style::default().fg(Color::DarkGray),
                        },
                    ),
                }
            };

            let x = inner.x + LABEL_WIDTH + sc as u16 * COL_WIDTH;
            if x < inner.x + inner.width {
                let buf = frame.buffer_mut();
                buf.set_string(x, display_y, &ch, style);
            }
        }

        display_y += 1;
    }
}

fn draw_section_header(frame: &mut Frame, inner: Rect, y: u16, label: &str) {
    let width = inner.width as usize;
    // Fill with dashes after the label text, e.g. "── bases ────────────────"
    let text = format!("{:─<width$}", format!("{} ", label), width = width);
    let buf = frame.buffer_mut();
    buf.set_string(inner.x, y, &text, Style::default().fg(Color::DarkGray));
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    if app.input_mode == InputMode::CreatingProject {
        let text = format!(
            "New project name: {}█   [Enter to create, Esc to cancel]",
            app.input_buffer
        );
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
            area,
        );
        return;
    }
    if app.input_mode == InputMode::EditingInterface {
        let text = format!(
            "Interface name: {}█   [Enter to save, Esc to cancel]",
            app.input_buffer
        );
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
            area,
        );
        return;
    }
    let cursor_info = app
        .cols
        .get(app.cursor_col)
        .zip(app.rows.get(app.cursor_row))
        .map(|(col, row)| format!("  [{}/{}]", col.name, row.name))
        .unwrap_or_default();
    let fold_hint = if app.fold_active {
        "  [f] unfold"
    } else {
        ""
    };
    let status_text = format!("{}{}{}", app.status, cursor_info, fold_hint);
    frame.render_widget(
        Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn truncate(s: &str, max: usize) -> &str {
    s.char_indices().nth(max).map_or(s, |(i, _)| &s[..i])
}

