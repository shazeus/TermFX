use ratatui::text::Line;

use crate::core::timeline::{Timeline, TrackKind};

pub fn timeline_lines(timeline: &Timeline, width: u16) -> Vec<Line<'static>> {
    let width = width.saturating_sub(2).max(24) as usize;
    let label_width = 8usize;
    let canvas_width = width.saturating_sub(label_width + 2).max(10);
    let duration = timeline.duration_frames().max(1);
    let mut lines = vec![Line::from(time_ruler(canvas_width, label_width))];

    for track in &timeline.tracks {
        let mut cells = vec!['.'; canvas_width];
        for clip in &track.clips {
            let start = scale(clip.start_frame, duration, canvas_width);
            let end = scale(clip.end_frame(), duration, canvas_width).max(start + 1);
            let fill = match track.kind {
                TrackKind::Video => '#',
                TrackKind::Audio => '=',
            };

            for cell in cells.iter_mut().take(end.min(canvas_width)).skip(start) {
                *cell = fill;
            }

            write_clip_name(&mut cells, start, end.min(canvas_width), &clip.name);
        }

        let marker = if track.locked {
            "L"
        } else if track.muted {
            "M"
        } else {
            " "
        };
        let label = format!("{:<label_width$}", format!("{}{}", track.name, marker));
        lines.push(Line::from(format!(
            "{}|{}",
            label,
            cells.into_iter().collect::<String>()
        )));
    }

    lines
}

fn time_ruler(canvas_width: usize, label_width: usize) -> String {
    let mut cells = vec!['-'; canvas_width];
    if let Some(first) = cells.first_mut() {
        *first = '0';
    }
    if canvas_width > 2 {
        cells[canvas_width / 2] = '|';
        cells[canvas_width - 1] = '>';
    }
    format!(
        "{:<label_width$}|{}",
        "time",
        cells.into_iter().collect::<String>()
    )
}

fn scale(frame: u64, duration: u64, width: usize) -> usize {
    ((frame as f64 / duration as f64) * width as f64).floor() as usize
}

fn write_clip_name(cells: &mut [char], start: usize, end: usize, name: &str) {
    if end <= start + 2 {
        return;
    }

    let available = end - start;
    for (offset, ch) in name.chars().take(available).enumerate() {
        cells[start + offset] = ch;
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::core::timeline::{Clip, Timeline, TrackKind};

    use super::*;

    #[test]
    fn timeline_lines_show_clip_name() {
        let mut timeline = Timeline::default();
        timeline
            .add_clip(
                0,
                Clip::media("intro", Uuid::new_v4(), TrackKind::Video, 0, 120),
            )
            .unwrap();

        let rendered = timeline_lines(&timeline, 80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("intro"));
    }
}
