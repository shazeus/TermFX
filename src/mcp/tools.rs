use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::core::effect::{Effect, effect_library};
use crate::core::media::AssetKind;
use crate::core::smart::{SmartEditMode, plan_smart_edit};
use crate::core::time::Frame;
use crate::error::{Result, TermFxError};
use crate::project::Project;
use crate::render::ffmpeg::{RenderSettings, build_ffmpeg_command};

pub struct ToolRegistry {
    project_path: PathBuf,
    project: Arc<Mutex<Project>>,
}

impl ToolRegistry {
    pub fn new(project_path: PathBuf, project: Arc<Mutex<Project>>) -> Self {
        Self {
            project_path,
            project,
        }
    }

    pub fn list_tools(&self) -> Value {
        let effect_names = effect_library()
            .iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        json!({
            "tools": [
                {
                    "name": "list_media",
                    "description": "List raw media assets and timeline clips in the current TermFX project.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                },
                {
                    "name": "list_effects",
                    "description": "List the built-in TermFX effect library with categories and short descriptions.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                },
                {
                    "name": "import_media",
                    "description": "Register a raw video, audio, or image asset in the project media pool.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "kind": { "type": "string", "enum": ["video", "audio", "image"], "default": "video" },
                            "name": { "type": "string" }
                        },
                        "required": ["path"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "append_media",
                    "description": "Append a media asset to a timeline track as a clip.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "media_id": { "type": "string", "format": "uuid" },
                            "track": { "type": "integer", "minimum": 0, "default": 0 },
                            "start_seconds": { "type": "number", "minimum": 0, "default": 0 },
                            "duration_seconds": { "type": "number", "exclusiveMinimum": 0 }
                        },
                        "required": ["media_id", "duration_seconds"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "add_text_clip",
                    "description": "Create a dedicated text clip on a video track.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "track": { "type": "integer", "minimum": 0, "default": 0 },
                            "text": { "type": "string" },
                            "start_seconds": { "type": "number", "minimum": 0, "default": 0 },
                            "duration_seconds": { "type": "number", "exclusiveMinimum": 0, "default": 2 }
                        },
                        "required": ["text"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "cut_video",
                    "description": "Trim a clip to a source range or remove a timeline range with optional ripple delete.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "mode": {
                                "type": "string",
                                "enum": ["remove_range", "trim_clip"],
                                "default": "remove_range"
                            },
                            "clip_id": { "type": "string", "format": "uuid" },
                            "start_seconds": { "type": "number", "minimum": 0 },
                            "end_seconds": { "type": "number", "exclusiveMinimum": 0 },
                            "ripple": { "type": "boolean", "default": true }
                        },
                        "required": ["start_seconds", "end_seconds"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "apply_effect",
                    "description": "Apply a compositor effect to a clip. Call list_effects to inspect the full library and parameter intent.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "effect": {
                                "type": "string",
                                "enum": effect_names
                            },
                            "params": {
                                "type": "object",
                                "additionalProperties": true
                            }
                        },
                        "required": ["clip_id", "effect"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "remove_effect",
                    "description": "Remove an effect instance from a clip.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "effect_id": { "type": "string", "format": "uuid" }
                        },
                        "required": ["clip_id", "effect_id"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "set_effect_enabled",
                    "description": "Enable or disable an existing effect instance without deleting it.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "effect_id": { "type": "string", "format": "uuid" },
                            "enabled": { "type": "boolean" }
                        },
                        "required": ["clip_id", "effect_id", "enabled"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "update_clip",
                    "description": "Update basic clip timing and mix parameters.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "name": { "type": "string" },
                            "start_seconds": { "type": "number", "minimum": 0 },
                            "duration_seconds": { "type": "number", "exclusiveMinimum": 0 },
                            "trim_start_seconds": { "type": "number", "minimum": 0 },
                            "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
                            "volume": { "type": "number", "minimum": 0 }
                        },
                        "required": ["clip_id"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "move_clip",
                    "description": "Move a clip to another compatible track and optionally reposition it in time.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "track": { "type": "integer", "minimum": 0 },
                            "start_seconds": { "type": "number", "minimum": 0 }
                        },
                        "required": ["clip_id", "track"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "split_clip",
                    "description": "Split a clip at an absolute timeline timestamp.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "at_seconds": { "type": "number", "exclusiveMinimum": 0 }
                        },
                        "required": ["clip_id", "at_seconds"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "remove_clip",
                    "description": "Delete a clip from the timeline.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" }
                        },
                        "required": ["clip_id"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "set_timeline_settings",
                    "description": "Update render dimensions, FPS, or audio sample rate for the current project.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "width": { "type": "integer", "minimum": 16 },
                            "height": { "type": "integer", "minimum": 16 },
                            "fps": { "type": "number", "exclusiveMinimum": 0 },
                            "sample_rate": { "type": "integer", "minimum": 8000 }
                        },
                        "additionalProperties": false
                    }
                },
                {
                    "name": "render_command",
                    "description": "Build and return the FFmpeg render command without executing it.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "output": { "type": "string", "default": "out.mp4" }
                        },
                        "additionalProperties": false
                    }
                },
                {
                    "name": "smart_edit",
                    "description": "Create an analysis plan for silence-based jump cuts or beat-sync editing.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "mode": {
                                "type": "string",
                                "enum": ["silence", "beat_sync"]
                            },
                            "threshold_db": { "type": "number", "default": -35 },
                            "min_silence_seconds": { "type": "number", "default": 0.35 },
                            "dry_run": { "type": "boolean", "default": true }
                        },
                        "required": ["mode"],
                        "additionalProperties": false
                    }
                }
            ]
        })
    }

    pub async fn call_tool(&self, params: Value) -> Result<Value> {
        let params: ToolCallParams = serde_json::from_value(params)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        match params.name.as_str() {
            "list_media" => self.list_media().await,
            "list_effects" => self.list_effects().await,
            "import_media" => {
                self.import_media(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "append_media" => {
                self.append_media(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "add_text_clip" => {
                self.add_text_clip(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "cut_video" => {
                self.cut_video(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "apply_effect" => {
                self.apply_effect(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "remove_effect" => {
                self.remove_effect(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "set_effect_enabled" => {
                self.set_effect_enabled(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "update_clip" => {
                self.update_clip(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "move_clip" => {
                self.move_clip(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "split_clip" => {
                self.split_clip(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "remove_clip" => {
                self.remove_clip(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "set_timeline_settings" => {
                self.set_timeline_settings(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "render_command" => {
                self.render_command(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            "smart_edit" => {
                self.smart_edit(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            other => Err(TermFxError::InvalidMcpRequest(format!(
                "unknown tool: {other}"
            ))),
        }
    }

    async fn list_effects(&self) -> Result<Value> {
        Ok(tool_text(json!({
            "effects": effect_library()
        })))
    }

    async fn import_media(&self, args: Value) -> Result<Value> {
        let args: ImportMediaArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let kind = parse_asset_kind(args.kind.as_deref().unwrap_or("video"))?;
        let asset = project.add_media(args.path, kind, args.name);
        project.save(&self.project_path)?;

        Ok(tool_text(json!({
            "status": "ok",
            "asset": asset
        })))
    }

    async fn append_media(&self, args: Value) -> Result<Value> {
        let args: AppendMediaArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let start_frame = project
            .timeline
            .fps
            .frames_from_seconds(args.start_seconds.unwrap_or(0.0));
        let duration_frames = project
            .timeline
            .fps
            .frames_from_seconds(args.duration_seconds);
        let track = args.track.unwrap_or(0);
        let clip_id = project.add_media_clip(args.media_id, track, start_frame, duration_frames)?;
        project.save(&self.project_path)?;

        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": clip_id,
            "media_id": args.media_id,
            "track": track,
            "start_frame": start_frame,
            "duration_frames": duration_frames
        })))
    }

    async fn add_text_clip(&self, args: Value) -> Result<Value> {
        let args: AddTextClipArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let start_frame = project
            .timeline
            .fps
            .frames_from_seconds(args.start_seconds.unwrap_or(0.0));
        let duration_frames = project
            .timeline
            .fps
            .frames_from_seconds(args.duration_seconds.unwrap_or(2.0));
        let track = args.track.unwrap_or(0);
        let clip_id = project.add_text_clip(track, args.text, start_frame, duration_frames)?;
        project.save(&self.project_path)?;

        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": clip_id,
            "track": track,
            "start_frame": start_frame,
            "duration_frames": duration_frames
        })))
    }

    async fn list_media(&self) -> Result<Value> {
        let project = self.project.lock().await;
        Ok(tool_text(json!({
            "media": project.media,
            "timeline": project.timeline,
        })))
    }

    async fn cut_video(&self, args: Value) -> Result<Value> {
        let args: CutVideoArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let start_frame = project.timeline.fps.frames_from_seconds(args.start_seconds);
        let end_frame = project.timeline.fps.frames_from_seconds(args.end_seconds);

        match args.mode.as_deref().unwrap_or("remove_range") {
            "trim_clip" => {
                let clip_id = args.clip_id.ok_or_else(|| {
                    TermFxError::InvalidMcpRequest("clip_id is required for trim_clip".to_string())
                })?;
                project
                    .timeline
                    .trim_clip_to_source_range(clip_id, start_frame, end_frame)?;
                project.save(&self.project_path)?;
                Ok(tool_text(json!({
                    "status": "ok",
                    "operation": "trim_clip",
                    "clip_id": clip_id,
                    "source_start_frame": start_frame,
                    "source_end_frame": end_frame
                })))
            }
            "remove_range" => {
                project.timeline.remove_timeline_range(
                    start_frame,
                    end_frame,
                    args.ripple.unwrap_or(true),
                )?;
                project.save(&self.project_path)?;
                Ok(tool_text(json!({
                    "status": "ok",
                    "operation": "remove_range",
                    "start_frame": start_frame,
                    "end_frame": end_frame,
                    "ripple": args.ripple.unwrap_or(true)
                })))
            }
            other => Err(TermFxError::InvalidMcpRequest(format!(
                "unsupported cut mode: {other}"
            ))),
        }
    }

    async fn apply_effect(&self, args: Value) -> Result<Value> {
        let args: ApplyEffectArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let effect = parse_effect(&project, &args)?;
        let effect_id = project.apply_effect(args.clip_id, effect)?;
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": args.clip_id,
            "effect_id": effect_id
        })))
    }

    async fn remove_effect(&self, args: Value) -> Result<Value> {
        let args: EffectRefArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        project.remove_effect(args.clip_id, args.effect_id)?;
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": args.clip_id,
            "effect_id": args.effect_id
        })))
    }

    async fn set_effect_enabled(&self, args: Value) -> Result<Value> {
        let args: SetEffectEnabledArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        project.set_effect_enabled(args.clip_id, args.effect_id, args.enabled)?;
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": args.clip_id,
            "effect_id": args.effect_id,
            "enabled": args.enabled
        })))
    }

    async fn update_clip(&self, args: Value) -> Result<Value> {
        let args: UpdateClipArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let fps = project.timeline.fps;
        let clip = project.timeline.clip_mut(args.clip_id)?;

        if let Some(name) = args.name {
            clip.name = name;
        }
        if let Some(start_seconds) = args.start_seconds {
            clip.start_frame = fps.frames_from_seconds(start_seconds);
        }
        if let Some(duration_seconds) = args.duration_seconds {
            clip.duration_frames = fps.frames_from_seconds(duration_seconds);
        }
        if let Some(trim_start_seconds) = args.trim_start_seconds {
            clip.trim_start_frame = fps.frames_from_seconds(trim_start_seconds);
        }
        if let Some(opacity) = args.opacity {
            clip.opacity = opacity.clamp(0.0, 1.0);
        }
        if let Some(volume) = args.volume {
            clip.volume = volume.max(0.0);
        }

        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": args.clip_id
        })))
    }

    async fn move_clip(&self, args: Value) -> Result<Value> {
        let args: MoveClipArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        project
            .timeline
            .move_clip_to_track(args.clip_id, args.track)?;
        if let Some(start_seconds) = args.start_seconds {
            let start_frame = project.timeline.fps.frames_from_seconds(start_seconds);
            project.timeline.clip_mut(args.clip_id)?.start_frame = start_frame;
        }
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "clip_id": args.clip_id,
            "track": args.track
        })))
    }

    async fn split_clip(&self, args: Value) -> Result<Value> {
        let args: SplitClipArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let split_frame = project.timeline.fps.frames_from_seconds(args.at_seconds);
        let right_clip_id = project
            .timeline
            .split_clip_at_timeline_frame(args.clip_id, split_frame)?;
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "left_clip_id": args.clip_id,
            "right_clip_id": right_clip_id,
            "split_frame": split_frame
        })))
    }

    async fn remove_clip(&self, args: Value) -> Result<Value> {
        let args: ClipRefArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        let removed = project.timeline.remove_clip(args.clip_id)?;
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "removed_clip": removed
        })))
    }

    async fn set_timeline_settings(&self, args: Value) -> Result<Value> {
        let args: TimelineSettingsArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let mut project = self.project.lock().await;
        if let Some(width) = args.width {
            project.timeline.width = width;
        }
        if let Some(height) = args.height {
            project.timeline.height = height;
        }
        if let Some(fps) = args.fps {
            project.timeline.fps = fps_from_float(fps)?;
        }
        if let Some(sample_rate) = args.sample_rate {
            project.timeline.sample_rate = sample_rate;
        }
        project.save(&self.project_path)?;
        Ok(tool_text(json!({
            "status": "ok",
            "timeline": project.timeline
        })))
    }

    async fn render_command(&self, args: Value) -> Result<Value> {
        let args: RenderCommandArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let project = self.project.lock().await;
        let output = args.output.unwrap_or_else(|| PathBuf::from("out.mp4"));
        let command = build_ffmpeg_command(
            &project,
            &output,
            RenderSettings::from_timeline(&project.timeline),
        )?;
        Ok(tool_text(json!({
            "command": command.display_shell(),
            "filtergraph": command.filtergraph,
            "args": command.args
        })))
    }

    async fn smart_edit(&self, args: Value) -> Result<Value> {
        let args: SmartEditArgs = serde_json::from_value(args)
            .map_err(|error| TermFxError::InvalidMcpRequest(error.to_string()))?;
        let project = self.project.lock().await;
        let mode = match args.mode.as_str() {
            "silence" => SmartEditMode::Silence,
            "beat_sync" => SmartEditMode::BeatSync,
            other => {
                return Err(TermFxError::InvalidMcpRequest(format!(
                    "unsupported smart edit mode: {other}"
                )));
            }
        };
        let plan = plan_smart_edit(
            &project,
            mode,
            args.threshold_db,
            args.min_silence_seconds,
            args.dry_run.unwrap_or(true),
        );
        Ok(tool_text(json!(plan)))
    }
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ImportMediaArgs {
    path: PathBuf,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppendMediaArgs {
    media_id: Uuid,
    #[serde(default)]
    track: Option<usize>,
    #[serde(default)]
    start_seconds: Option<f64>,
    duration_seconds: f64,
}

#[derive(Debug, Deserialize)]
struct AddTextClipArgs {
    text: String,
    #[serde(default)]
    track: Option<usize>,
    #[serde(default)]
    start_seconds: Option<f64>,
    #[serde(default)]
    duration_seconds: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CutVideoArgs {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    clip_id: Option<Uuid>,
    start_seconds: f64,
    end_seconds: f64,
    #[serde(default)]
    ripple: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ApplyEffectArgs {
    clip_id: Uuid,
    effect: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct EffectRefArgs {
    clip_id: Uuid,
    effect_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct SetEffectEnabledArgs {
    clip_id: Uuid,
    effect_id: Uuid,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateClipArgs {
    clip_id: Uuid,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    start_seconds: Option<f64>,
    #[serde(default)]
    duration_seconds: Option<f64>,
    #[serde(default)]
    trim_start_seconds: Option<f64>,
    #[serde(default)]
    opacity: Option<f32>,
    #[serde(default)]
    volume: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct MoveClipArgs {
    clip_id: Uuid,
    track: usize,
    #[serde(default)]
    start_seconds: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SplitClipArgs {
    clip_id: Uuid,
    at_seconds: f64,
}

#[derive(Debug, Deserialize)]
struct ClipRefArgs {
    clip_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct TimelineSettingsArgs {
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    fps: Option<f64>,
    #[serde(default)]
    sample_rate: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RenderCommandArgs {
    #[serde(default)]
    output: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct SmartEditArgs {
    mode: String,
    #[serde(default)]
    threshold_db: Option<f32>,
    #[serde(default)]
    min_silence_seconds: Option<f32>,
    #[serde(default)]
    dry_run: Option<bool>,
}

fn parse_effect(project: &Project, args: &ApplyEffectArgs) -> Result<Effect> {
    let params = &args.params;
    let fps = project.timeline.fps;
    let effect = match args.effect.as_str() {
        "black_and_white" => Effect::BlackWhite,
        "sepia" => Effect::Sepia,
        "invert" => Effect::Invert,
        "edge_detect" => Effect::EdgeDetect,
        "glitch" => Effect::Glitch {
            intensity: number(params, "intensity").unwrap_or(0.5) as f32,
        },
        "brightness_contrast" => Effect::BrightnessContrast {
            brightness: number(params, "brightness").unwrap_or(0.0) as f32,
            contrast: number(params, "contrast").unwrap_or(1.0) as f32,
            saturation: number(params, "saturation").unwrap_or(1.0) as f32,
        },
        "hue_rotate" => Effect::HueRotate {
            degrees: number(params, "degrees").unwrap_or(30.0) as f32,
        },
        "gaussian_blur" => Effect::GaussianBlur {
            sigma: number(params, "sigma").unwrap_or(6.0) as f32,
        },
        "box_blur" => Effect::BoxBlur {
            radius: number(params, "radius").unwrap_or(4.0).round() as u32,
        },
        "sharpen" => Effect::Sharpen {
            amount: number(params, "amount").unwrap_or(1.0) as f32,
        },
        "vignette" => Effect::Vignette {
            angle: number(params, "angle").unwrap_or(0.7) as f32,
        },
        "film_grain" => Effect::FilmGrain {
            strength: number(params, "strength").unwrap_or(0.35) as f32,
        },
        "pixelate" => Effect::Pixelate {
            block_size: number(params, "block_size").unwrap_or(12.0).round() as u32,
        },
        "chromatic_aberration" => Effect::ChromaticAberration {
            offset_px: number(params, "offset_px").unwrap_or(6.0).round() as i32,
        },
        "lens_distortion" => Effect::LensDistortion {
            k1: number(params, "k1").unwrap_or(-0.12) as f32,
            k2: number(params, "k2").unwrap_or(0.02) as f32,
        },
        "posterize" => Effect::Posterize {
            levels: number(params, "levels").unwrap_or(8.0).round() as u8,
        },
        "letterbox" => Effect::Letterbox {
            height_px: number(params, "height_px").unwrap_or(120.0).round() as u32,
            color: string(params, "color", "black"),
        },
        "border" => Effect::Border {
            thickness_px: number(params, "thickness_px").unwrap_or(8.0).round() as u32,
            color: string(params, "color", "white"),
        },
        "fade_in" => Effect::FadeIn {
            duration_frames: seconds_param(params, "duration_seconds", 0.35, fps),
        },
        "fade_out" => Effect::FadeOut {
            duration_frames: seconds_param(params, "duration_seconds", 0.35, fps),
        },
        "s_shake" => Effect::SShake {
            amplitude_px: number(params, "amplitude_px").unwrap_or(12.0).round() as u32,
            frequency_hz: number(params, "frequency_hz").unwrap_or(9.0) as f32,
            seed: number(params, "seed").unwrap_or(0.0) as f32,
        },
        "text_overlay" => Effect::TextOverlay {
            text: params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            x: number(params, "x").unwrap_or(80.0).round() as i32,
            y: number(params, "y").unwrap_or(80.0).round() as i32,
            font_size: number(params, "font_size").unwrap_or(48.0).round() as u32,
            color: params
                .get("color")
                .and_then(Value::as_str)
                .unwrap_or("white")
                .to_string(),
            start_frame: seconds_param(params, "start_seconds", 0.0, fps),
            duration_frames: seconds_param(params, "duration_seconds", 2.0, fps),
        },
        other => {
            return Err(TermFxError::InvalidMcpRequest(format!(
                "unsupported effect: {other}"
            )));
        }
    };
    Ok(effect)
}

fn parse_asset_kind(kind: &str) -> Result<AssetKind> {
    match kind {
        "video" => Ok(AssetKind::Video),
        "audio" => Ok(AssetKind::Audio),
        "image" => Ok(AssetKind::Image),
        other => Err(TermFxError::InvalidMcpRequest(format!(
            "unsupported media kind: {other}"
        ))),
    }
}

fn fps_from_float(fps: f64) -> Result<crate::core::time::Fps> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(TermFxError::InvalidMcpRequest(
            "fps must be a positive finite number".to_string(),
        ));
    }

    Ok(crate::core::time::Fps::new(
        (fps * 1_000.0).round() as u32,
        1_000,
    ))
}

fn seconds_param(params: &Value, key: &str, default: f64, fps: crate::core::time::Fps) -> Frame {
    fps.frames_from_seconds(number(params, key).unwrap_or(default))
}

fn number(params: &Value, key: &str) -> Option<f64> {
    params.get(key).and_then(Value::as_f64)
}

fn string(params: &Value, key: &str, default: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn tool_text(value: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
            }
        ],
        "isError": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_list_exposes_required_tools() {
        let project = Arc::new(Mutex::new(Project::new("demo", PathBuf::from("."))));
        let registry = ToolRegistry::new(PathBuf::from("demo.json"), project);
        let list = registry.list_tools();
        let names = list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert!(names.contains(&"list_media"));
        assert!(names.contains(&"list_effects"));
        assert!(names.contains(&"import_media"));
        assert!(names.contains(&"append_media"));
        assert!(names.contains(&"add_text_clip"));
        assert!(names.contains(&"cut_video"));
        assert!(names.contains(&"apply_effect"));
        assert!(names.contains(&"remove_effect"));
        assert!(names.contains(&"set_effect_enabled"));
        assert!(names.contains(&"update_clip"));
        assert!(names.contains(&"move_clip"));
        assert!(names.contains(&"split_clip"));
        assert!(names.contains(&"remove_clip"));
        assert!(names.contains(&"set_timeline_settings"));
        assert!(names.contains(&"render_command"));
        assert!(names.contains(&"smart_edit"));
    }

    #[test]
    fn effect_library_exposes_expanded_effect_set() {
        let names = effect_library()
            .into_iter()
            .map(|effect| effect.name)
            .collect::<Vec<_>>();

        assert!(names.len() >= 20);
        assert!(names.contains(&"sepia"));
        assert!(names.contains(&"vignette"));
        assert!(names.contains(&"pixelate"));
        assert!(names.contains(&"chromatic_aberration"));
        assert!(names.contains(&"s_shake"));
    }
}
