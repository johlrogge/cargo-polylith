use std::env;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::tui::{app::App, ui};
use crate::workspace::{build_workspace_map, resolve_root};

pub fn run(workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;
    let map = build_workspace_map(&root)?;

    if map.projects.is_empty() {
        anyhow::bail!(
            "no projects found — run `cargo polylith project new <name>` first"
        );
    }

    let mut app = App::new(&map)?;

    // Set up terminal
    enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("entering alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("creating terminal")?;

    let result = run_loop(&mut terminal, &mut app);

    // Restore terminal unconditionally
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
            // Ignore key-release events on Windows
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.quit = true;
                }
                KeyCode::Tab => app.toggle_focus(),
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Char(' ') => app.toggle_base(),
                KeyCode::Char('w') => {
                    if let Err(e) = app.write_all() {
                        app.status = format!("error: {e:#}");
                    }
                }
                _ => {}
            }
        }

        if app.quit {
            break;
        }
    }
    Ok(())
}
