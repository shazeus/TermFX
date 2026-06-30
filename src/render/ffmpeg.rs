use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use uuid::Uuid;

use crate::core::effect::{Effect, EffectInstance};
use crate::core::media::AssetKind;
use crate::core::time::{Fps, Frame};
use crate::core::timeline::{Clip, ClipSource, Timeline, TrackKind};
use crate::error::{Result, TermFxError};
use crate::project::Project;

use super::filtergraph::{escape_drawtext, seconds};

#[derive(Clone, Debug)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    pub sample_rate: u32,
    pub background_color: String,
    pub video_codec: String,
    pub audio_codec: String,
}

impl RenderSettings {
    pub fn from_timeline(timeline: &Timeline) -> Self {
        Self {
            width: timeline.width,
            height: timeline.height,
            fps: timeline.fps,
            sample_rate: timeline.sample_rate,
            background_color: "black".to_string(),
            video_codec: "libx264".to_string(),
            audio_codec: "aac".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfmpegCommand {
    pub program: String,
    pub args: Vec<String>,
    pub filtergraph: String,
}

impl FfmpegCommand {
    pub fn display_shell(&self) -> String {
        let mut pieces = vec![self.program.clone()];
        pieces.extend(self.args.iter().map(|arg| shell_quote(arg)));
        pieces.join(" ")
    }

    pub fn spawn_and_wait(&self) -> Result<()> {
        let status = Command::new(&self.program).args(&self.args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(TermFxError::FfmpegFailed(status))
        }
    }
}

pub fn build_ffmpeg_command(
    project: &Project,
    output: &Path,
    settings: RenderSettings,
) -> Result<FfmpegCommand> {
    let input_media_ids = collect_input_media_ids(project);
    let input_index = input_media_ids
        .iter()
        .enumerate()
        .map(|(index, media_id)| (*media_id, index))
        .collect::<HashMap<_, _>>();

    let mut args = vec!["-hide_banner".to_string(), "-y".to_string()];
    for media_id in &input_media_ids {
        let asset = project
            .media
            .iter()
            .find(|asset| asset.id == *media_id)
            .ok_or(TermFxError::MissingMedia(*media_id))?;
        args.push("-i".to_string());
        args.push(asset.path.display().to_string());
    }

    let mut graph = Vec::new();
    let timeline_duration_frames = project
        .timeline
        .duration_frames()
        .max(settings.fps.frames_from_seconds(0.1));
    let timeline_duration = settings.fps.seconds_from_frames(timeline_duration_frames);
    let base_label = "vbase";
    graph.push(format!(
        "color=c={}:s={}x{}:r={}:d={:.6}[{}]",
        settings.background_color,
        settings.width,
        settings.height,
        settings.fps.expression(),
        timeline_duration,
        base_label
    ));

    let mut previous_label = base_label.to_string();
    for (clip_number, clip) in sorted_video_clips(&project.timeline)
        .into_iter()
        .enumerate()
    {
        let clip_label = format!("vclip{}", clip_number);
        graph.push(build_video_clip_chain(
            clip,
            clip_number,
            &input_index,
            &settings,
            &clip_label,
        )?);

        let next_label = format!("vstack{}", clip_number);
        graph.push(format!(
            "[{}][{}]overlay=x=0:y=0:eof_action=pass:shortest=0[{}]",
            previous_label, clip_label, next_label
        ));
        previous_label = next_label;
    }

    graph.push(format!("[{}]format=yuv420p[vout]", previous_label));
    let audio_label = build_audio_graph(
        project,
        &input_index,
        &settings,
        timeline_duration,
        &mut graph,
    )?;

    let filtergraph = graph.join(";");
    args.push("-filter_complex".to_string());
    args.push(filtergraph.clone());
    args.push("-map".to_string());
    args.push("[vout]".to_string());
    args.push("-map".to_string());
    args.push(format!("[{}]", audio_label));
    args.push("-c:v".to_string());
    args.push(settings.video_codec);
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    args.push("-c:a".to_string());
    args.push(settings.audio_codec);
    args.push("-movflags".to_string());
    args.push("+faststart".to_string());
    args.push(output.display().to_string());

    Ok(FfmpegCommand {
        program: "ffmpeg".to_string(),
        args,
        filtergraph,
    })
}

fn collect_input_media_ids(project: &Project) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    let mut media_ids = Vec::new();
    for track in &project.timeline.tracks {
        for clip in &track.clips {
            if let ClipSource::Media { media_id } = clip.source {
                if seen.insert(media_id) {
                    media_ids.push(media_id);
                }
            }
        }
    }
    media_ids
}

fn sorted_video_clips(timeline: &Timeline) -> Vec<&Clip> {
    let mut clips = timeline
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Video && !track.muted)
        .flat_map(|track| {
            track
                .clips
                .iter()
                .filter(move |clip| !track.locked && clip.track_kind == TrackKind::Video)
                .map(move |clip| (track.index, clip))
        })
        .collect::<Vec<_>>();

    clips.sort_by_key(|(track_index, clip)| (*track_index, clip.start_frame, clip.id));
    clips.into_iter().map(|(_, clip)| clip).collect()
}

fn build_video_clip_chain(
    clip: &Clip,
    clip_number: usize,
    input_index: &HashMap<Uuid, usize>,
    settings: &RenderSettings,
    output_label: &str,
) -> Result<String> {
    let duration = seconds(settings.fps, clip.duration_frames);
    let timeline_start = seconds(settings.fps, clip.start_frame);
    let mut filters = match &clip.source {
        ClipSource::Media { media_id } => {
            let index = input_index
                .get(media_id)
                .ok_or(TermFxError::MissingMedia(*media_id))?;
            vec![
                format!(
                    "[{}:v]trim=start={:.6}:duration={:.6}",
                    index,
                    seconds(settings.fps, clip.trim_start_frame),
                    duration
                ),
                "setpts=PTS-STARTPTS".to_string(),
                format!(
                    "scale={}x{}:force_original_aspect_ratio=decrease",
                    settings.width, settings.height
                ),
                format!(
                    "pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black@0",
                    settings.width, settings.height
                ),
                "format=rgba".to_string(),
            ]
        }
        ClipSource::Text { text } => vec![
            format!(
                "color=c=black@0:s={}x{}:r={}:d={:.6}",
                settings.width,
                settings.height,
                settings.fps.expression(),
                duration
            ),
            "format=rgba".to_string(),
            format!(
                "drawtext=text='{}':x=(w-text_w)/2:y=(h-text_h)/2:fontsize=64:fontcolor=white",
                escape_drawtext(text)
            ),
        ],
    };

    for effect in clip.effects.iter().filter(|effect| effect.enabled) {
        append_effect_filters(&mut filters, effect, settings, clip.duration_frames);
    }

    if (clip.opacity - 1.0).abs() > f32::EPSILON {
        filters.push(format!(
            "colorchannelmixer=aa={:.3}",
            clip.opacity.clamp(0.0, 1.0)
        ));
    }

    filters.push(format!("setpts=PTS+{:.6}/TB", timeline_start));

    let mut chain = filters.join(",");
    if !chain.starts_with('[') {
        chain = format!("{}", chain);
    }
    Ok(format!("{}[{}]", chain, output_label).replace(&format!("[vclip{}][", clip_number), "["))
}

fn append_effect_filters(
    filters: &mut Vec<String>,
    instance: &EffectInstance,
    settings: &RenderSettings,
    clip_duration_frames: Frame,
) {
    match &instance.effect {
        Effect::BlackWhite => filters.push("hue=s=0".to_string()),
        Effect::Glitch { intensity } => {
            let amount = (intensity.clamp(0.0, 1.0) * 12.0).max(1.0);
            let noise = (intensity.clamp(0.0, 1.0) * 40.0).max(3.0);
            filters.push(format!(
                "rgbashift=rh={:.1}:gv=-{:.1}:bh={:.1}",
                amount,
                amount / 2.0,
                amount / 3.0
            ));
            filters.push(format!("noise=alls={:.1}:allf=t+u", noise));
        }
        Effect::FadeIn { duration_frames } => filters.push(format!(
            "fade=t=in:st=0:d={:.6}:alpha=1",
            seconds(settings.fps, *duration_frames)
        )),
        Effect::FadeOut { duration_frames } => {
            let fade_duration = seconds(settings.fps, *duration_frames);
            let clip_duration = seconds(settings.fps, clip_duration_frames);
            let start = (clip_duration - fade_duration).max(0.0);
            filters.push(format!(
                "fade=t=out:st={:.6}:d={:.6}:alpha=1",
                start, fade_duration
            ));
        }
        Effect::SShake {
            amplitude_px,
            frequency_hz,
            seed,
        } => {
            let amp = (*amplitude_px).max(1);
            filters.push(format!(
                "scale={}x{}",
                settings.width + amp * 2,
                settings.height + amp * 2
            ));
            filters.push(format!(
                "crop={}:{}:x='{}+{}*sin(2*PI*{:.3}*t+{:.3})':y='{}+{}*cos(2*PI*{:.3}*t+{:.3})'",
                settings.width,
                settings.height,
                amp,
                amp,
                frequency_hz,
                seed,
                amp,
                amp,
                frequency_hz * 1.37,
                seed + 1.9
            ));
        }
        Effect::TextOverlay {
            text,
            x,
            y,
            font_size,
            color,
            start_frame,
            duration_frames,
        } => {
            let start = seconds(settings.fps, *start_frame);
            let end = seconds(settings.fps, start_frame + duration_frames);
            filters.push(format!(
                "drawtext=text='{}':x={}:y={}:fontsize={}:fontcolor={}:enable='between(t,{:.6},{:.6})'",
                escape_drawtext(text),
                x,
                y,
                font_size,
                color,
                start,
                end
            ));
        }
    }
}

fn build_audio_graph(
    project: &Project,
    input_index: &HashMap<Uuid, usize>,
    settings: &RenderSettings,
    timeline_duration: f64,
    graph: &mut Vec<String>,
) -> Result<String> {
    let mut audio_labels = Vec::new();
    for (audio_number, clip) in sorted_audio_clips(project).into_iter().enumerate() {
        let ClipSource::Media { media_id } = clip.source else {
            continue;
        };
        let asset = project
            .media
            .iter()
            .find(|asset| asset.id == media_id)
            .ok_or(TermFxError::MissingMedia(media_id))?;
        if !asset.has_audio || asset.kind == AssetKind::Image {
            continue;
        }

        let input = input_index
            .get(&media_id)
            .ok_or(TermFxError::MissingMedia(media_id))?;
        let label = format!("a{}", audio_number);
        let delay_ms = (settings.fps.seconds_from_frames(clip.start_frame) * 1_000.0).round();
        graph.push(format!(
            "[{}:a]atrim=start={:.6}:duration={:.6},asetpts=PTS-STARTPTS,volume={:.3},adelay={}:all=1[{}]",
            input,
            seconds(settings.fps, clip.trim_start_frame),
            seconds(settings.fps, clip.duration_frames),
            clip.volume.max(0.0),
            delay_ms,
            label
        ));
        audio_labels.push(label);
    }

    if audio_labels.is_empty() {
        graph.push(format!(
            "anullsrc=channel_layout=stereo:sample_rate={}:d={:.6}[aout]",
            settings.sample_rate, timeline_duration
        ));
        return Ok("aout".to_string());
    }

    let inputs = audio_labels
        .iter()
        .map(|label| format!("[{}]", label))
        .collect::<String>();
    graph.push(format!(
        "{}amix=inputs={}:duration=longest:normalize=0[aout]",
        inputs,
        audio_labels.len()
    ));
    Ok("aout".to_string())
}

fn sorted_audio_clips(project: &Project) -> Vec<&Clip> {
    let mut clips = project
        .timeline
        .tracks
        .iter()
        .filter(|track| !track.muted)
        .flat_map(|track| track.clips.iter().map(move |clip| (track.index, clip)))
        .filter(|(_, clip)| matches!(clip.source, ClipSource::Media { .. }))
        .collect::<Vec<_>>();
    clips.sort_by_key(|(track_index, clip)| (*track_index, clip.start_frame, clip.id));
    clips.into_iter().map(|(_, clip)| clip).collect()
}

fn shell_quote(value: &str) -> String {
    if value.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '+' | '=')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::core::effect::Effect;
    use crate::core::media::AssetKind;

    use super::*;

    #[test]
    fn filtergraph_contains_compositor_effects_and_audio_mix() {
        let mut project = Project::new("demo", PathBuf::from("."));
        let asset = project.add_media(PathBuf::from("shot.mp4"), AssetKind::Video, None);
        let clip_id = project.add_media_clip(asset.id, 0, 0, 90).unwrap();
        project
            .apply_effect(
                clip_id,
                Effect::SShake {
                    amplitude_px: 12,
                    frequency_hz: 9.0,
                    seed: 0.25,
                },
            )
            .unwrap();
        project
            .apply_effect(
                clip_id,
                Effect::TextOverlay {
                    text: "Launch".to_string(),
                    x: 100,
                    y: 80,
                    font_size: 48,
                    color: "white".to_string(),
                    start_frame: 0,
                    duration_frames: 60,
                },
            )
            .unwrap();

        let command = build_ffmpeg_command(
            &project,
            Path::new("out.mp4"),
            RenderSettings::from_timeline(&project.timeline),
        )
        .unwrap();

        assert!(command.filtergraph.contains("crop=1920:1080"));
        assert!(command.filtergraph.contains("pad=1920:1080"));
        assert!(command.filtergraph.contains("drawtext=text='Launch'"));
        assert!(command.filtergraph.contains("amix=inputs=1"));
    }
}
