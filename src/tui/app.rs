use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::project::Project;

use super::layout::render;

pub struct AppState {
    pub project: Project,
    pub selected_track: usize,
    pub status: String,
}

impl AppState {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            selected_track: 0,
            status: "MCP: idle | q: quit | up/down: select track".to_string(),
        }
    }
}

pub fn run(project: Project) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, AppState::new(project));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: AppState,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| render(frame, &app))?;

        if event::poll(Duration::from_millis(150))? {
            match event::read()? {
                Event::Key(key) if key.code == KeyCode::Char('q') => break,
                Event::Key(key) if key.code == KeyCode::Down => {
                    app.selected_track = (app.selected_track + 1)
                        .min(app.project.timeline.tracks.len().saturating_sub(1));
                }
                Event::Key(key) if key.code == KeyCode::Up => {
                    app.selected_track = app.selected_track.saturating_sub(1);
                }
                _ => {}
            }
        }
    }

    Ok(())
}
