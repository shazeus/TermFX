use serde::{Deserialize, Serialize};

use crate::project::Project;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartEditMode {
    Silence,
    BeatSync,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SmartEditPlan {
    pub mode: SmartEditMode,
    pub dry_run: bool,
    pub summary: String,
    pub ffmpeg_analysis_args: Vec<String>,
}

pub fn plan_smart_edit(
    project: &Project,
    mode: SmartEditMode,
    threshold_db: Option<f32>,
    min_silence_seconds: Option<f32>,
    dry_run: bool,
) -> SmartEditPlan {
    let threshold = threshold_db.unwrap_or(-35.0);
    let min_silence = min_silence_seconds.unwrap_or(0.35);
    let first_audio_path = project
        .media
        .iter()
        .find(|asset| asset.has_audio)
        .map(|asset| asset.path.display().to_string())
        .unwrap_or_else(|| "<no-audio-media>".to_string());

    let ffmpeg_analysis_args = match mode {
        SmartEditMode::Silence => vec![
            "-hide_banner".to_string(),
            "-i".to_string(),
            first_audio_path,
            "-af".to_string(),
            format!("silencedetect=noise={}dB:d={}", threshold, min_silence),
            "-f".to_string(),
            "null".to_string(),
            "-".to_string(),
        ],
        SmartEditMode::BeatSync => vec![
            "-hide_banner".to_string(),
            "-i".to_string(),
            first_audio_path,
            "-af".to_string(),
            "astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level"
                .to_string(),
            "-f".to_string(),
            "null".to_string(),
            "-".to_string(),
        ],
    };

    SmartEditPlan {
        mode,
        dry_run,
        summary:
            "analysis plan created; production path parses FFmpeg stderr into timeline cut markers"
                .to_string(),
        ffmpeg_analysis_args,
    }
}
