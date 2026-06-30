use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use uuid::Uuid;

use crate::core::effect::{
    Effect, EffectInstance, KeyframeEasing, KeyframeProperty, TransformKeyframe,
};
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
        let x_expression = overlay_keyframe_expression(clip, KeyframeProperty::X, &settings, 0.0);
        let y_expression = overlay_keyframe_expression(clip, KeyframeProperty::Y, &settings, 0.0);
        graph.push(format!(
            "[{}][{}]overlay=x='{}':y='{}':eval=frame:eof_action=pass:shortest=0[{}]",
            previous_label, clip_label, x_expression, y_expression, next_label
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
            let trim_duration = duration * clip.speed.max(0.01) as f64;
            vec![
                format!(
                    "[{}:v]trim=start={:.6}:duration={:.6}",
                    index,
                    seconds(settings.fps, clip.trim_start_frame),
                    trim_duration
                ),
                format!("setpts=(PTS-STARTPTS)/{:.6}", clip.speed.max(0.01)),
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

    append_keyframed_transform_filters(&mut filters, clip, settings);

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

fn append_keyframed_transform_filters(
    filters: &mut Vec<String>,
    clip: &Clip,
    settings: &RenderSettings,
) {
    if has_keyframes_for(clip, KeyframeProperty::Scale) {
        let scale = local_keyframe_expression(clip, KeyframeProperty::Scale, settings, 1.0);
        filters.push(format!(
            "scale=w='trunc(iw*({})/2)*2':h='trunc(ih*({})/2)*2':eval=frame",
            scale, scale
        ));
    }

    if has_keyframes_for(clip, KeyframeProperty::RotationDegrees) {
        let rotation =
            local_keyframe_expression(clip, KeyframeProperty::RotationDegrees, settings, 0.0);
        filters.push(format!(
            "rotate=a='({})*PI/180':c=black@0:ow=iw:oh=ih",
            rotation
        ));
    }
}

fn overlay_keyframe_expression(
    clip: &Clip,
    property: KeyframeProperty,
    settings: &RenderSettings,
    default: f32,
) -> String {
    keyframe_expression(
        &clip.keyframes,
        property,
        settings,
        default,
        seconds(settings.fps, clip.start_frame),
    )
}

fn local_keyframe_expression(
    clip: &Clip,
    property: KeyframeProperty,
    settings: &RenderSettings,
    default: f32,
) -> String {
    keyframe_expression(&clip.keyframes, property, settings, default, 0.0)
}

fn has_keyframes_for(clip: &Clip, property: KeyframeProperty) -> bool {
    clip.keyframes.iter().any(|keyframe| {
        (keyframe.value(property) - default_keyframe_value(property)).abs() > f32::EPSILON
    })
}

fn default_keyframe_value(property: KeyframeProperty) -> f32 {
    match property {
        KeyframeProperty::Scale | KeyframeProperty::Opacity | KeyframeProperty::Volume => 1.0,
        KeyframeProperty::X | KeyframeProperty::Y | KeyframeProperty::RotationDegrees => 0.0,
    }
}

fn keyframe_expression(
    keyframes: &[TransformKeyframe],
    property: KeyframeProperty,
    settings: &RenderSettings,
    default: f32,
    time_offset_seconds: f64,
) -> String {
    if keyframes.is_empty() {
        return format_float(default as f64);
    }

    let mut sorted = keyframes.to_vec();
    sorted.sort_by_key(|keyframe| (keyframe.frame, keyframe.id));
    if sorted.len() == 1 {
        return format_float(sorted[0].value(property) as f64);
    }

    let first_time = time_offset_seconds + seconds(settings.fps, sorted[0].frame);
    let first_value = sorted[0].value(property) as f64;
    let mut expression = format_float(
        sorted
            .last()
            .map(|keyframe| keyframe.value(property) as f64)
            .unwrap_or(default as f64),
    );

    for pair in sorted.windows(2).rev() {
        let start = &pair[0];
        let end = &pair[1];
        let start_time = time_offset_seconds + seconds(settings.fps, start.frame);
        let end_time = time_offset_seconds + seconds(settings.fps, end.frame);
        let start_value = start.value(property) as f64;
        let end_value = end.value(property) as f64;
        let segment = segment_expression(
            "t",
            start_time,
            end_time,
            start_value,
            end_value,
            start.easing,
        );
        expression = format!("if(lte(t,{end_time:.6}),{segment},{expression})");
    }

    format!(
        "if(lte(t,{first_time:.6}),{},{} )",
        format_float(first_value),
        expression
    )
    .replace(" ", "")
}

fn segment_expression(
    time_variable: &str,
    start_time: f64,
    end_time: f64,
    start_value: f64,
    end_value: f64,
    easing: KeyframeEasing,
) -> String {
    let duration = (end_time - start_time).max(0.000_001);
    if easing == KeyframeEasing::Hold {
        return format_float(start_value);
    }

    let progress = format!("(({time_variable}-{start_time:.6})/{duration:.6})");
    let eased = match easing {
        KeyframeEasing::Hold => "0".to_string(),
        KeyframeEasing::Linear => progress,
        KeyframeEasing::EaseIn => format!("pow({progress},2)"),
        KeyframeEasing::EaseOut => format!("1-pow(1-({progress}),2)"),
        KeyframeEasing::EaseInOut => {
            format!("if(lt({0},0.5),2*pow({0},2),1-pow(-2*{0}+2,2)/2)", progress)
        }
    };

    format!(
        "{}+({})*({})",
        format_float(start_value),
        format_float(end_value - start_value),
        eased
    )
}

fn format_float(value: f64) -> String {
    format!("{value:.6}")
}

fn append_effect_filters(
    filters: &mut Vec<String>,
    instance: &EffectInstance,
    settings: &RenderSettings,
    clip_duration_frames: Frame,
) {
    match &instance.effect {
        Effect::BlackWhite => filters.push("hue=s=0".to_string()),
        Effect::Sepia => filters.push(
            "colorchannelmixer=rr=.393:rg=.769:rb=.189:gr=.349:gg=.686:gb=.168:br=.272:bg=.534:bb=.131".to_string(),
        ),
        Effect::Invert => filters.push("negate".to_string()),
        Effect::EdgeDetect => {
            filters.push("edgedetect=mode=colormix:high=0.22:low=0.08".to_string())
        }
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
        Effect::BrightnessContrast {
            brightness,
            contrast,
            saturation,
        } => filters.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            brightness.clamp(-1.0, 1.0),
            contrast.clamp(0.0, 4.0),
            saturation.clamp(0.0, 4.0)
        )),
        Effect::HueRotate { degrees } => filters.push(format!("hue=h={:.3}", degrees)),
        Effect::GaussianBlur { sigma } => {
            filters.push(format!("gblur=sigma={:.3}", sigma.clamp(0.1, 50.0)))
        }
        Effect::BoxBlur { radius } => {
            let radius = (*radius).clamp(1, 64);
            filters.push(format!("boxblur=lr={}:lp=1:cr={}:cp=1", radius, radius));
        }
        Effect::Sharpen { amount } => {
            filters.push(format!("unsharp=5:5:{:.3}:5:5:0.0", amount.clamp(0.0, 5.0)))
        }
        Effect::Vignette { angle } => {
            filters.push(format!("vignette=angle={:.3}", angle.clamp(0.0, 3.14)))
        }
        Effect::FilmGrain { strength } => filters.push(format!(
            "noise=alls={:.1}:allf=t+u",
            (strength.clamp(0.0, 1.0) * 60.0).max(1.0)
        )),
        Effect::Pixelate { block_size } => {
            let block = (*block_size).clamp(2, 128);
            filters.push(format!(
                "scale=iw/{0}:ih/{0}:flags=neighbor,scale=iw*{0}:ih*{0}:flags=neighbor",
                block
            ));
        }
        Effect::ChromaticAberration { offset_px } => {
            let offset = (*offset_px).clamp(-64, 64);
            filters.push(format!(
                "rgbashift=rh={}:rv=0:gh=0:gv=0:bh={}:bv=0",
                offset, -offset
            ));
        }
        Effect::LensDistortion { k1, k2 } => filters.push(format!(
            "lenscorrection=k1={:.4}:k2={:.4}",
            k1.clamp(-1.0, 1.0),
            k2.clamp(-1.0, 1.0)
        )),
        Effect::Posterize { levels } => {
            let levels = (*levels).clamp(2, 64) as u32;
            let step = (256 / levels).max(1);
            filters.push(format!(
                "lutrgb=r='floor(val/{0})*{0}':g='floor(val/{0})*{0}':b='floor(val/{0})*{0}'",
                step
            ));
        }
        Effect::Letterbox { height_px, color } => {
            let height = (*height_px).min(settings.height / 2);
            let color = sanitize_color(color);
            filters.push(format!(
                "drawbox=x=0:y=0:w=iw:h={height}:color={color}:t=fill,drawbox=x=0:y=ih-{height}:w=iw:h={height}:color={color}:t=fill"
            ));
        }
        Effect::Border {
            thickness_px,
            color,
        } => {
            let thickness = (*thickness_px).clamp(1, 256);
            let color = sanitize_color(color);
            filters.push(format!(
                "drawbox=x=0:y=0:w=iw:h=ih:color={color}:t={thickness}"
            ));
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

fn sanitize_color(color: &str) -> String {
    let sanitized = color
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '#' | '@' | '.' | '_'))
        .collect::<String>();

    if sanitized.is_empty() {
        "white".to_string()
    } else {
        sanitized
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
        let trim_duration =
            seconds(settings.fps, clip.duration_frames) * clip.speed.max(0.01) as f64;
        let atempo = atempo_filter_chain(clip.speed.max(0.01));
        graph.push(format!(
            "[{}:a]atrim=start={:.6}:duration={:.6},asetpts=PTS-STARTPTS{},volume={:.3},adelay={}:all=1[{}]",
            input,
            seconds(settings.fps, clip.trim_start_frame),
            trim_duration,
            atempo,
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

fn atempo_filter_chain(speed: f32) -> String {
    let mut remaining = speed.clamp(0.01, 100.0);
    let mut filters = Vec::new();

    while remaining > 2.0 {
        filters.push("atempo=2.0".to_string());
        remaining /= 2.0;
    }

    while remaining < 0.5 {
        filters.push("atempo=0.5".to_string());
        remaining /= 0.5;
    }

    filters.push(format!("atempo={:.6}", remaining));
    filters
        .into_iter()
        .map(|filter| format!(",{filter}"))
        .collect::<String>()
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

    use crate::core::effect::{Effect, TransformKeyframe};
    use crate::core::media::AssetKind;

    use super::*;

    #[test]
    fn filtergraph_contains_compositor_effects_and_audio_mix() {
        let mut project = Project::new("demo", PathBuf::from("."));
        let asset = project.add_media(PathBuf::from("shot.mp4"), AssetKind::Video, None);
        let clip_id = project.add_media_clip(asset.id, 0, 0, 90).unwrap();
        let mut start = TransformKeyframe::new(0);
        start.x = 0.0;
        start.scale = 1.0;
        let mut end = TransformKeyframe::new(30);
        end.x = 120.0;
        end.scale = 0.8;
        project
            .timeline
            .clip_mut(clip_id)
            .unwrap()
            .add_keyframe(start);
        project
            .timeline
            .clip_mut(clip_id)
            .unwrap()
            .add_keyframe(end);
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
        assert!(command.filtergraph.contains("overlay=x='if("));
        assert!(command.filtergraph.contains("eval=frame"));
        assert!(command.filtergraph.contains("drawtext=text='Launch'"));
        assert!(command.filtergraph.contains("amix=inputs=1"));
    }
}
