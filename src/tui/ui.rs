use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::app::{App, Focus};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Outer layout: main area + status bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    draw_main(frame, app, outer[0]);
    draw_status(frame, app, outer[1]);
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    draw_projects(frame, app, chunks[0]);
    draw_bases(frame, app, chunks[1]);
}

fn draw_projects(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Projects;
    let block = Block::default()
        .title(" Projects ")
        .borders(Borders::ALL)
        .border_style(border_style(focused));

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let marker = if p.modified { "*" } else { " " };
            let label = format!("{marker} {}", p.name);
            let style = if i == app.proj_idx && focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if i == app.proj_idx {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.proj_idx));

    frame.render_stateful_widget(
        List::new(items).block(block).highlight_symbol("▶ "),
        area,
        &mut state,
    );
}

fn draw_bases(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Bases;
    let title = app
        .current_project()
        .map(|p| format!(" Bases — {} ", p.name))
        .unwrap_or_else(|| " Bases ".into());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style(focused));

    let selections = app
        .current_project()
        .map(|p| p.base_selections.as_slice())
        .unwrap_or(&[]);

    if selections.is_empty() {
        frame.render_widget(
            Paragraph::new("(no bases in workspace)").block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = selections
        .iter()
        .enumerate()
        .map(|(i, (name, selected))| {
            let checkbox = if *selected { "[x]" } else { "[ ]" };
            let style = if i == app.base_idx && focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if i == app.base_idx {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(format!("{checkbox} "), style),
                Span::styled(name.clone(), style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.base_idx));

    frame.render_stateful_widget(
        List::new(items).block(block).highlight_symbol("  "),
        area,
        &mut state,
    );
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let style = Style::default().fg(Color::DarkGray);
    frame.render_widget(Paragraph::new(app.status.as_str()).style(style), area);
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
