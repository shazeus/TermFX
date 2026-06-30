use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::core::timeline::ClipSource;

use super::app::AppState;
use super::timeline_widget::timeline_lines;

pub fn render(frame: &mut Frame, app: &AppState) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(12),
            Constraint::Length(8),
        ])
        .split(area);

    frame.render_widget(header(app), rows[0]);

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(24),
            Constraint::Percentage(46),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

    frame.render_widget(media_pool(app), middle[0]);
    frame.render_widget(preview(app), middle[1]);
    frame.render_widget(inspector(app), middle[2]);
    frame.render_widget(timeline(app, rows[2].width), rows[2]);
    frame.render_widget(status(app), rows[3]);
}

fn header(app: &AppState) -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled(
            "TermFX",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  project={}  fps={}  mcp={}",
            app.project.name,
            app.project.timeline.fps.expression(),
            app.mcp_endpoint.as_deref().unwrap_or("off")
        )),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Project"))
}

fn media_pool(app: &AppState) -> Paragraph<'static> {
    let lines = if app.project.media.is_empty() {
        vec![Line::from("No media. Use `termfx add-media`.")]
    } else {
        app.project
            .media
            .iter()
            .map(|asset| {
                Line::from(format!(
                    "{}  {:?}  {}",
                    asset.name,
                    asset.kind,
                    asset.path.display()
                ))
            })
            .collect()
    };

    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Project Assets"),
        )
        .wrap(Wrap { trim: true })
}

fn preview(app: &AppState) -> Paragraph<'static> {
    let duration = app
        .project
        .timeline
        .fps
        .seconds_from_frames(app.project.timeline.duration_frames());
    let preview = vec![
        Line::from("+---------------- preview ----------------+"),
        Line::from("| ASCII preview placeholder               |"),
        Line::from("| Production path: mpv IPC or sixel/kitty |"),
        Line::from("| frame cache + audio waveform thumbnails |"),
        Line::from("+-----------------------------------------+"),
        Line::from(format!("Timeline duration: {:.2}s", duration)),
    ];

    Paragraph::new(preview)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Video Preview"),
        )
        .wrap(Wrap { trim: false })
}

fn inspector(app: &AppState) -> Paragraph<'static> {
    let selected_track = app.project.timeline.tracks.get(app.selected_track);
    let mut lines = Vec::new();
    if let Some(track) = selected_track {
        lines.push(Line::from(format!(
            "Track: {} ({:?})",
            track.name, track.kind
        )));
        lines.push(Line::from(format!(
            "Muted: {}  Locked: {}",
            track.muted, track.locked
        )));
        lines.push(Line::from(""));
        for clip in &track.clips {
            let source = match &clip.source {
                ClipSource::Media { media_id } => format!("media:{media_id}"),
                ClipSource::Text { text } => format!("text:{text}"),
            };
            lines.push(Line::from(format!(
                "{}  start={}  dur={}  {} fx",
                clip.name,
                clip.start_frame,
                clip.duration_frames,
                clip.effects.len()
            )));
            lines.push(Line::from(source));
        }
    }

    Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Inspector"))
        .wrap(Wrap { trim: true })
}

fn timeline(app: &AppState, width: u16) -> Paragraph<'static> {
    Paragraph::new(timeline_lines(&app.project.timeline, width))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Timeline & Layers"),
        )
        .wrap(Wrap { trim: false })
}

fn status(app: &AppState) -> Paragraph<'static> {
    let mut lines = vec![
        Line::from(format!(
            "Endpoint: {}",
            app.mcp_endpoint
                .as_deref()
                .unwrap_or("disabled; start stdio MCP with `termfx mcp`")
        )),
        Line::from(format!("Project: {}", app.project_path.display())),
        Line::from(format!("Log: {}", app.mcp_log_path.display())),
        Line::from(format!("Status: {}", app.status)),
    ];

    for event in &app.mcp_events {
        lines.push(Line::from(event.clone()));
    }

    Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("AI / MCP"))
        .wrap(Wrap { trim: true })
}
