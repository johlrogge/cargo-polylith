use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

use super::app::{App, DepState, InputMode, RowKind};

const LABEL_WIDTH: u16 = 24;
const COL_WIDTH: u16 = 2; // cell char + space

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    draw_grid(frame, app, layout[0]);
    draw_status(frame, app, layout[1]);
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

    // ── Data rows ──────────────────────────────────────────────────────────
    let mut display_y = inner.y + header_rows;
    let bottom = inner.y + inner.height;

    for row_i in scroll_row..app.rows.len() {
        let row_kind = app.rows[row_i].kind;

        // Section header before first component (only when scroll_row is in or before component section)
        if row_i == scroll_row && row_i < n_components && n_components > 0 {
            if display_y < bottom {
                draw_section_header(frame, inner, display_y, "── components");
                display_y += 1;
            }
        }

        // Section header before first base
        if row_i == n_components && n_bases > 0 {
            if display_y < bottom {
                draw_section_header(frame, inner, display_y, "── bases");
                display_y += 1;
            }
        }

        if display_y >= bottom {
            break;
        }

        let is_cursor_row = row_i == app.cursor_row;

        // Row label
        let label = truncate(&app.rows[row_i].name, (LABEL_WIDTH - 1) as usize);
        let padded = format!("{:<width$}", label, width = (LABEL_WIDTH - 1) as usize);
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
            buf.set_string(inner.x, display_y, &padded, label_style);
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

            let ch = match dep_state {
                DepState::Direct => "x",
                DepState::Transitive => "·",
                DepState::None => "-",
            };
            let style = if is_cursor {
                Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)
            } else {
                match dep_state {
                    DepState::Direct => {
                        let base = Style::default().fg(Color::Green);
                        if modified { base.add_modifier(Modifier::BOLD) } else { base }
                    }
                    DepState::Transitive => Style::default().fg(Color::Gray),
                    DepState::None => Style::default().fg(Color::DarkGray),
                }
            };

            let x = inner.x + LABEL_WIDTH + sc as u16 * COL_WIDTH;
            if x < inner.x + inner.width {
                let buf = frame.buffer_mut();
                buf.set_string(x, display_y, ch, style);
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
    let cursor_info = app
        .cols
        .get(app.cursor_col)
        .zip(app.rows.get(app.cursor_row))
        .map(|(col, row)| format!("  [{}/{}]", col.name, row.name))
        .unwrap_or_default();
    let text = format!("{}{}", app.status, cursor_info);
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

