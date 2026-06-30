use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::core::effect::Effect;
use crate::core::smart::{SmartEditMode, plan_smart_edit};
use crate::core::time::Frame;
use crate::error::{Result, TermFxError};
use crate::project::Project;

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
                    "description": "Apply compositor effects such as black_and_white, glitch, fade_in, fade_out, s_shake, and text_overlay to a clip.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "clip_id": { "type": "string", "format": "uuid" },
                            "effect": {
                                "type": "string",
                                "enum": ["black_and_white", "glitch", "fade_in", "fade_out", "s_shake", "text_overlay"]
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
            "append_media" => {
                self.append_media(params.arguments.unwrap_or_else(|| json!({})))
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
            "smart_edit" => {
                self.smart_edit(params.arguments.unwrap_or_else(|| json!({})))
                    .await
            }
            other => Err(TermFxError::InvalidMcpRequest(format!(
                "unknown tool: {other}"
            ))),
        }
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
struct AppendMediaArgs {
    media_id: Uuid,
    #[serde(default)]
    track: Option<usize>,
    #[serde(default)]
    start_seconds: Option<f64>,
    duration_seconds: f64,
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
        "glitch" => Effect::Glitch {
            intensity: number(params, "intensity").unwrap_or(0.5) as f32,
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

fn seconds_param(params: &Value, key: &str, default: f64, fps: crate::core::time::Fps) -> Frame {
    fps.frames_from_seconds(number(params, key).unwrap_or(default))
}

fn number(params: &Value, key: &str) -> Option<f64> {
    params.get(key).and_then(Value::as_f64)
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
        assert!(names.contains(&"append_media"));
        assert!(names.contains(&"cut_video"));
        assert!(names.contains(&"apply_effect"));
        assert!(names.contains(&"smart_edit"));
    }
}
