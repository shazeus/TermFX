use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::{fs, time::Instant};

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde_json::Value;

use crate::mcp::server::run_http_server;
use crate::mcp::tools::mcp_log_path;
use crate::project::Project;

use super::layout::render;

const PROJECT_RELOAD_INTERVAL: Duration = Duration::from_millis(700);
const MCP_LOG_LINES: usize = 5;

pub struct AppState {
    pub project: Project,
    pub project_path: PathBuf,
    pub selected_track: usize,
    pub status: String,
    pub mcp_endpoint: Option<String>,
    pub mcp_log_path: PathBuf,
    pub mcp_events: Vec<String>,
    last_refresh: Instant,
}

impl AppState {
    pub fn new(project_path: PathBuf, project: Project, mcp_endpoint: Option<String>) -> Self {
        let mcp_log_path = mcp_log_path(&project_path);
        let mut app = Self {
            project,
            project_path,
            selected_track: 0,
            status: String::new(),
            mcp_endpoint,
            mcp_log_path,
            mcp_events: Vec::new(),
            last_refresh: Instant::now() - PROJECT_RELOAD_INTERVAL,
        };
        app.refresh_from_disk();
        app
    }

    pub fn refresh_from_disk(&mut self) {
        if self.last_refresh.elapsed() < PROJECT_RELOAD_INTERVAL {
            return;
        }

        match Project::load(&self.project_path) {
            Ok(project) => {
                self.project = project;
                self.selected_track = self
                    .selected_track
                    .min(self.project.timeline.tracks.len().saturating_sub(1));
                self.status = "Project synced from disk".to_string();
            }
            Err(error) => {
                self.status = format!("Project reload failed: {error}");
            }
        }

        self.mcp_events = read_mcp_events(&self.mcp_log_path, MCP_LOG_LINES);
        self.last_refresh = Instant::now();
    }
}

pub fn run(project_path: PathBuf, start_mcp_http: bool, mcp_port: u16) -> io::Result<()> {
    let project = load_or_create_project(&project_path)?;
    let mcp_endpoint = if start_mcp_http {
        start_embedded_mcp(project_path.clone(), mcp_port);
        Some(format!("http://127.0.0.1:{mcp_port}/mcp"))
    } else {
        None
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(
        &mut terminal,
        AppState::new(project_path, project, mcp_endpoint),
    );

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
        app.refresh_from_disk();
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

fn load_or_create_project(project_path: &Path) -> io::Result<Project> {
    if project_path.exists() {
        return Project::load(project_path).map_err(to_io_error);
    }

    if let Some(parent) = project_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let name = project_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("termfx")
        .to_string();
    let project = Project::new(name, std::env::current_dir()?);
    project.save(project_path).map_err(to_io_error)?;
    Ok(project)
}

fn start_embedded_mcp(project_path: PathBuf, port: u16) {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("TermFX MCP runtime failed: {error}");
                return;
            }
        };

        let address = SocketAddr::from(([127, 0, 0, 1], port));
        if let Err(error) = runtime.block_on(run_http_server(project_path, address)) {
            eprintln!("TermFX MCP server failed: {error}");
        }
    });
}

fn read_mcp_events(path: &Path, limit: usize) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return vec!["No AI tool calls yet.".to_string()];
    };

    let mut events = contents
        .lines()
        .rev()
        .filter_map(format_mcp_event)
        .take(limit)
        .collect::<Vec<_>>();
    events.reverse();

    if events.is_empty() {
        vec!["No AI tool calls yet.".to_string()]
    } else {
        events
    }
}

fn format_mcp_event(line: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    let tool = value
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let prefix = match status {
        "started" => "RUN",
        "ok" => "OK ",
        "error" => "ERR",
        other => other,
    };

    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Some(format!("{prefix} {tool}: {error}"));
    }

    let arguments = value
        .get("arguments")
        .filter(|arguments| !arguments.is_null())
        .map(compact_json)
        .unwrap_or_default();

    if arguments.is_empty() {
        Some(format!("{prefix} {tool}"))
    } else {
        Some(format!("{prefix} {tool} {arguments}"))
    }
}

fn compact_json(value: &Value) -> String {
    let text = value.to_string();
    let max_chars = 90;
    if text.chars().count() <= max_chars {
        return text;
    }

    let mut truncated = text.chars().take(max_chars - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn to_io_error(error: impl std::error::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, error.to_string())
}
