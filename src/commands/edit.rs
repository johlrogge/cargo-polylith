use std::env;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::tui::{app::{App, InputMode}, ui};
use crate::workspace::{build_workspace_map, resolve_root};

pub fn run(workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;
    let mut app = App::new(&map)?;

    enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("entering alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("creating terminal")?;

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match app.input_mode {
                InputMode::CreatingProject => match key.code {
                    KeyCode::Char(c) => app.input_char(c),
                    KeyCode::Backspace => app.input_backspace(),
                    KeyCode::Enter => {
                        if let Err(e) = app.confirm_create_project() {
                            app.status = format!("error: {e:#}");
                            app.input_mode = InputMode::Normal;
                            app.input_buffer.clear();
                        }
                    }
                    KeyCode::Esc => app.cancel_input(),
                    _ => {}
                },
                InputMode::EditingInterface => match key.code {
                    KeyCode::Enter => app.confirm_edit_interface(),
                    KeyCode::Esc => app.cancel_input(),
                    KeyCode::Char(c) => app.input_char(c),
                    KeyCode::Backspace => app.input_backspace(),
                    _ => {}
                },
                InputMode::Normal => {
                    let is_q = key.code == KeyCode::Char('q');
                    if !is_q {
                        app.confirm_quit = false;
                    }
                    if key.code != KeyCode::Char('g') {
                        app.pending_g = false;
                    }
                    match key.code {
                        KeyCode::Char('q') => {
                            let dirty = app.modified_cols.iter().any(|&m| m)
                                || app.modified_rows.iter().any(|&m| m);
                            if dirty && !app.confirm_quit {
                                app.status = "Unsaved changes — press w to save or q again to quit".into();
                                app.confirm_quit = true;
                            } else {
                                app.quit = true;
                            }
                        }
                        KeyCode::Esc => {
                            app.status = String::new();
                        }
                        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                        KeyCode::Left | KeyCode::Char('h') => app.move_left(),
                        KeyCode::Right | KeyCode::Char('l') => app.move_right(),
                        KeyCode::Char(' ') => app.toggle_cell(),
                        KeyCode::Char('w') => {
                            if let Err(e) = app.write_all() {
                                app.status = format!("error: {e:#}");
                            }
                        }
                        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.start_create_project();
                        }
                        KeyCode::Char('g') => {
                            if app.pending_g {
                                app.cursor_row = 0;
                                app.pending_g = false;
                            } else {
                                app.pending_g = true;
                            }
                        }
                        KeyCode::Char('G') => {
                            if !app.rows.is_empty() {
                                app.cursor_row = app.rows.len() - 1;
                            }
                        }
                        KeyCode::Char('i') => app.start_edit_interface(),
                        _ => {}
                    }
                }
            }
        }

        if app.quit {
            break;
        }
    }
    Ok(())
}
