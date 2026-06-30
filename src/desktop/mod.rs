use std::collections::BTreeMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Button, CentralPanel, Color32, ComboBox, Context, DragValue, FontId, Frame,
    Layout, Panel, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2, ViewportBuilder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::core::effect::{Effect, KeyframeEasing, TransformKeyframe, effect_library};
use crate::core::media::{AssetKind, MediaAsset};
use crate::core::time::{Fps, Frame as TimelineFrame};
use crate::core::timeline::{Clip, Timeline, TrackKind};
use crate::mcp::server::run_http_server;
use crate::mcp::tools::mcp_log_path;
use crate::project::Project;
use crate::render::ffmpeg::{RenderSettings, build_ffmpeg_command};

const APP_TITLE: &str = "TermFX Studio";
const CONFIG_FILE: &str = "studio-config.json";
const DEFAULT_MCP_PORT: u16 = 4739;
const LOG_REFRESH_INTERVAL: Duration = Duration::from_millis(750);

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1180.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|context| Ok(Box::new(StudioApp::new(context)))),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceMode {
    Home,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectorTab {
    Clip,
    Effects,
    Keyframes,
    Project,
    Render,
    Automation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectPreset {
    BlackWhite,
    Sepia,
    Invert,
    BrightnessContrast,
    Vignette,
    FilmGrain,
    Pixelate,
    GaussianBlur,
    Sharpen,
    FadeIn,
    FadeOut,
    SShake,
    TextOverlay,
}

impl EffectPreset {
    fn label(self) -> &'static str {
        match self {
            EffectPreset::BlackWhite => "Black and white",
            EffectPreset::Sepia => "Sepia",
            EffectPreset::Invert => "Invert",
            EffectPreset::BrightnessContrast => "Brightness / contrast",
            EffectPreset::Vignette => "Vignette",
            EffectPreset::FilmGrain => "Film grain",
            EffectPreset::Pixelate => "Pixelate",
            EffectPreset::GaussianBlur => "Gaussian blur",
            EffectPreset::Sharpen => "Sharpen",
            EffectPreset::FadeIn => "Fade in",
            EffectPreset::FadeOut => "Fade out",
            EffectPreset::SShake => "Shake",
            EffectPreset::TextOverlay => "Text overlay",
        }
    }

    fn effect(self, fps: Fps) -> Effect {
        match self {
            EffectPreset::BlackWhite => Effect::BlackWhite,
            EffectPreset::Sepia => Effect::Sepia,
            EffectPreset::Invert => Effect::Invert,
            EffectPreset::BrightnessContrast => Effect::BrightnessContrast {
                brightness: 0.02,
                contrast: 1.08,
                saturation: 1.12,
            },
            EffectPreset::Vignette => Effect::Vignette { angle: 0.75 },
            EffectPreset::FilmGrain => Effect::FilmGrain { strength: 0.25 },
            EffectPreset::Pixelate => Effect::Pixelate { block_size: 12 },
            EffectPreset::GaussianBlur => Effect::GaussianBlur { sigma: 4.0 },
            EffectPreset::Sharpen => Effect::Sharpen { amount: 1.0 },
            EffectPreset::FadeIn => Effect::FadeIn {
                duration_frames: fps.frames_from_seconds(0.45),
            },
            EffectPreset::FadeOut => Effect::FadeOut {
                duration_frames: fps.frames_from_seconds(0.45),
            },
            EffectPreset::SShake => Effect::SShake {
                amplitude_px: 18,
                frequency_hz: 10.0,
                seed: 7.0,
            },
            EffectPreset::TextOverlay => Effect::TextOverlay {
                text: "TITLE".to_string(),
                x: 80,
                y: 80,
                font_size: 64,
                color: "white".to_string(),
                start_frame: fps.frames_from_seconds(0.2),
                duration_frames: fps.frames_from_seconds(2.5),
            },
        }
    }
}

const EFFECT_PRESETS: &[EffectPreset] = &[
    EffectPreset::BlackWhite,
    EffectPreset::Sepia,
    EffectPreset::Invert,
    EffectPreset::BrightnessContrast,
    EffectPreset::Vignette,
    EffectPreset::FilmGrain,
    EffectPreset::Pixelate,
    EffectPreset::GaussianBlur,
    EffectPreset::Sharpen,
    EffectPreset::FadeIn,
    EffectPreset::FadeOut,
    EffectPreset::SShake,
    EffectPreset::TextOverlay,
];

#[derive(Default, Debug, Deserialize, Serialize)]
struct StudioConfig {
    recent_projects: Vec<PathBuf>,
}

#[derive(Debug)]
struct NewProjectForm {
    name: String,
    path: String,
    width: u32,
    height: u32,
    fps: f64,
    sample_rate: u32,
}

impl Default for NewProjectForm {
    fn default() -> Self {
        let path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("termfx.project.json");
        Self {
            name: "Untitled Project".to_string(),
            path: path.display().to_string(),
            width: 1_920,
            height: 1_080,
            fps: 30.0,
            sample_rate: 48_000,
        }
    }
}

#[derive(Debug)]
struct ImportForm {
    path: String,
    name: String,
    kind: AssetKind,
    duration_seconds: f64,
    target_track: usize,
    append_to_timeline: bool,
}

impl Default for ImportForm {
    fn default() -> Self {
        Self {
            path: String::new(),
            name: String::new(),
            kind: AssetKind::Video,
            duration_seconds: 5.0,
            target_track: 0,
            append_to_timeline: true,
        }
    }
}

#[derive(Debug)]
struct RenderForm {
    output_path: String,
    last_command: String,
    is_rendering: bool,
}

impl Default for RenderForm {
    fn default() -> Self {
        Self {
            output_path: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("termfx_render.mp4")
                .display()
                .to_string(),
            last_command: String::new(),
            is_rendering: false,
        }
    }
}

struct StudioApp {
    mode: WorkspaceMode,
    inspector_tab: InspectorTab,
    config: StudioConfig,
    config_path: PathBuf,
    project: Option<Project>,
    project_path: Option<PathBuf>,
    selected_asset: Option<Uuid>,
    selected_clip: Option<Uuid>,
    selected_effect: Option<Uuid>,
    selected_keyframe: Option<Uuid>,
    playhead_frame: TimelineFrame,
    zoom: f32,
    new_project: NewProjectForm,
    import_form: ImportForm,
    render_form: RenderForm,
    status: String,
    mcp_enabled: bool,
    mcp_port: u16,
    mcp_started: bool,
    mcp_log_path: Option<PathBuf>,
    mcp_events: Vec<String>,
    last_log_refresh: Instant,
}

impl StudioApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        configure_style(&context.egui_ctx);
        let config_path = config_path();
        let config = load_config(&config_path);
        Self {
            mode: WorkspaceMode::Home,
            inspector_tab: InspectorTab::Clip,
            config,
            config_path,
            project: None,
            project_path: None,
            selected_asset: None,
            selected_clip: None,
            selected_effect: None,
            selected_keyframe: None,
            playhead_frame: 0,
            zoom: 1.0,
            new_project: NewProjectForm::default(),
            import_form: ImportForm::default(),
            render_form: RenderForm::default(),
            status: "Ready".to_string(),
            mcp_enabled: true,
            mcp_port: DEFAULT_MCP_PORT,
            mcp_started: false,
            mcp_log_path: None,
            mcp_events: Vec::new(),
            last_log_refresh: Instant::now() - LOG_REFRESH_INTERVAL,
        }
    }

    fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    fn project_mut(&mut self) -> Option<&mut Project> {
        self.project.as_mut()
    }

    fn create_project(&mut self) {
        let path = PathBuf::from(self.new_project.path.trim());
        let mut project = Project::new(
            self.new_project.name.trim().to_string(),
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
        );
        project.timeline.width = self.new_project.width.max(16);
        project.timeline.height = self.new_project.height.max(16);
        project.timeline.fps = fps_from_float(self.new_project.fps);
        project.timeline.sample_rate = self.new_project.sample_rate.max(8_000);

        match project.save(&path) {
            Ok(()) => {
                self.load_project(path);
            }
            Err(error) => {
                self.status = format!("Project could not be created: {error}");
            }
        }
    }

    fn load_project(&mut self, path: PathBuf) {
        match Project::load(&path) {
            Ok(project) => {
                self.project = Some(project);
                self.project_path = Some(path.clone());
                self.mode = WorkspaceMode::Editor;
                self.selected_asset = None;
                self.selected_clip = first_clip_id(self.project.as_ref());
                self.selected_effect = None;
                self.selected_keyframe = None;
                self.playhead_frame = 0;
                self.mcp_log_path = Some(mcp_log_path(&path));
                self.status = format!("Opened {}", path.display());
                self.remember_project(path);
                if self.mcp_enabled {
                    self.start_mcp();
                }
            }
            Err(error) => {
                self.status = format!("Project could not be opened: {error}");
            }
        }
    }

    fn save_project(&mut self) {
        let Some(path) = self.project_path.clone() else {
            self.status = "No project path".to_string();
            return;
        };
        let Some(project) = self.project() else {
            self.status = "No project loaded".to_string();
            return;
        };
        match project.save(&path) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.remember_project(path);
            }
            Err(error) => {
                self.status = format!("Save failed: {error}");
            }
        }
    }

    fn remember_project(&mut self, path: PathBuf) {
        self.config
            .recent_projects
            .retain(|candidate| candidate != &path);
        self.config.recent_projects.insert(0, path);
        self.config.recent_projects.truncate(12);
        if let Err(error) = save_config(&self.config_path, &self.config) {
            self.status = format!("Recent projects could not be saved: {error}");
        }
    }

    fn maybe_reload_from_disk(&mut self) {
        let Some(path) = self.project_path.clone() else {
            return;
        };
        if self.last_log_refresh.elapsed() < LOG_REFRESH_INTERVAL {
            return;
        }
        self.last_log_refresh = Instant::now();

        if self.mcp_started {
            if let Ok(project) = Project::load(&path) {
                self.project = Some(project);
            }
        }

        if let Some(log_path) = &self.mcp_log_path {
            self.mcp_events = read_mcp_events(log_path, 10);
        }
    }

    fn start_mcp(&mut self) {
        if self.mcp_started {
            return;
        }
        let Some(path) = self.project_path.clone() else {
            self.status = "Open a project before starting MCP".to_string();
            return;
        };
        let port = self.mcp_port;
        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("MCP runtime failed: {error}");
                    return;
                }
            };
            let address = SocketAddr::from(([127, 0, 0, 1], port));
            if let Err(error) = runtime.block_on(run_http_server(path, address)) {
                eprintln!("MCP server failed: {error}");
            }
        });
        self.mcp_started = true;
        self.status = format!("MCP listening on http://127.0.0.1:{port}/mcp");
    }

    fn import_media(&mut self) {
        let path = PathBuf::from(self.import_form.path.trim());
        if path.as_os_str().is_empty() {
            self.status = "Choose a media file first".to_string();
            return;
        }
        let kind = self.import_form.kind.clone();
        let name = optional_text(&self.import_form.name);
        let duration_seconds = self.import_form.duration_seconds.max(0.1);
        let target_track = self.import_form.target_track;
        let append = self.import_form.append_to_timeline;

        let result = {
            let Some(project) = self.project_mut() else {
                self.status = "Open a project first".to_string();
                return;
            };

            let asset = project.add_media(path, kind, name);
            let asset_id = asset.id;
            let mut clip_id = None;
            let mut status = None;
            if append {
                let start_frame = next_track_start(&project.timeline, target_track);
                let duration_frames = project.timeline.fps.frames_from_seconds(duration_seconds);
                match project.add_media_clip(asset_id, target_track, start_frame, duration_frames) {
                    Ok(id) => {
                        clip_id = Some(id);
                    }
                    Err(error) => {
                        status = Some(format!("Media imported, but clip append failed: {error}"));
                    }
                }
            }
            (asset_id, clip_id, status)
        };
        self.selected_asset = Some(result.0);
        if let Some(clip_id) = result.1 {
            self.selected_clip = Some(clip_id);
        }
        if let Some(status) = result.2 {
            self.status = status;
        }
        self.save_project();
    }

    fn append_selected_asset(&mut self) {
        let Some(asset_id) = self.selected_asset else {
            self.status = "Select an asset first".to_string();
            return;
        };
        let target_track = self.import_form.target_track;
        let duration_seconds = self.import_form.duration_seconds.max(0.1);
        let Some(project) = self.project_mut() else {
            return;
        };
        let start_frame = next_track_start(&project.timeline, target_track);
        let duration_frames = project.timeline.fps.frames_from_seconds(duration_seconds);
        match project.add_media_clip(asset_id, target_track, start_frame, duration_frames) {
            Ok(clip_id) => {
                self.selected_clip = Some(clip_id);
                self.save_project();
            }
            Err(error) => {
                self.status = format!("Clip could not be appended: {error}");
            }
        }
    }

    fn add_text_clip(&mut self) {
        let target_track = self.import_form.target_track;
        let duration_seconds = self.import_form.duration_seconds.max(0.1);
        let Some(project) = self.project_mut() else {
            return;
        };
        let start_frame = next_track_start(&project.timeline, target_track);
        let duration_frames = project.timeline.fps.frames_from_seconds(duration_seconds);
        match project.add_text_clip(
            target_track,
            "Title".to_string(),
            start_frame,
            duration_frames,
        ) {
            Ok(clip_id) => {
                self.selected_clip = Some(clip_id);
                self.save_project();
            }
            Err(error) => {
                self.status = format!("Text clip could not be added: {error}");
            }
        }
    }

    fn selected_clip(&self) -> Option<&Clip> {
        self.project().and_then(|project| {
            self.selected_clip
                .and_then(|id| project.timeline.clip(id).ok())
        })
    }

    fn selected_clip_mut(&mut self) -> Option<&mut Clip> {
        let clip_id = self.selected_clip?;
        self.project_mut()?.timeline.clip_mut(clip_id).ok()
    }

    fn add_effect_to_selected_clip(&mut self, preset: EffectPreset) {
        let fps = self
            .project()
            .map(|project| project.timeline.fps)
            .unwrap_or_default();
        let Some(clip_id) = self.selected_clip else {
            self.status = "Select a clip first".to_string();
            return;
        };
        let Some(project) = self.project_mut() else {
            return;
        };
        match project.apply_effect(clip_id, preset.effect(fps)) {
            Ok(effect_id) => {
                self.selected_effect = Some(effect_id);
                self.inspector_tab = InspectorTab::Effects;
                self.save_project();
            }
            Err(error) => {
                self.status = format!("Effect could not be added: {error}");
            }
        }
    }

    fn remove_selected_effect(&mut self) {
        let (Some(clip_id), Some(effect_id)) = (self.selected_clip, self.selected_effect) else {
            return;
        };
        let Some(project) = self.project_mut() else {
            return;
        };
        match project.remove_effect(clip_id, effect_id) {
            Ok(()) => {
                self.selected_effect = None;
                self.save_project();
            }
            Err(error) => {
                self.status = format!("Effect could not be removed: {error}");
            }
        }
    }

    fn add_keyframe_at_playhead(&mut self) {
        let frame = self.playhead_frame;
        let Some(clip) = self.selected_clip_mut() else {
            self.status = "Select a clip first".to_string();
            return;
        };
        let mut keyframe = TransformKeyframe::new(frame);
        keyframe.x = 0.0;
        keyframe.y = 0.0;
        keyframe.scale = 1.0;
        keyframe.opacity = clip.opacity;
        keyframe.volume = clip.volume;
        keyframe.easing = KeyframeEasing::EaseInOut;
        self.selected_keyframe = Some(clip.add_keyframe(keyframe));
        self.inspector_tab = InspectorTab::Keyframes;
        self.save_project();
    }

    fn split_selected_clip(&mut self) {
        let Some(clip_id) = self.selected_clip else {
            return;
        };
        let playhead = self.playhead_frame;
        let Some(project) = self.project_mut() else {
            return;
        };
        match project
            .timeline
            .split_clip_at_timeline_frame(clip_id, playhead)
        {
            Ok(new_clip_id) => {
                self.selected_clip = Some(new_clip_id);
                self.save_project();
            }
            Err(error) => {
                self.status = format!("Split failed: {error}");
            }
        }
    }

    fn delete_selected_clip(&mut self) {
        let Some(clip_id) = self.selected_clip else {
            return;
        };
        let Some(project) = self.project_mut() else {
            return;
        };
        match project.timeline.remove_clip(clip_id) {
            Ok(_) => {
                self.selected_clip = first_clip_id(self.project.as_ref());
                self.save_project();
            }
            Err(error) => {
                self.status = format!("Delete failed: {error}");
            }
        }
    }

    fn add_marker_at_playhead(&mut self) {
        let playhead = self.playhead_frame;
        let Some(project) = self.project_mut() else {
            return;
        };
        project.timeline.add_marker(
            playhead,
            "Marker".to_string(),
            Some("cyan".to_string()),
            Some("Added from editor".to_string()),
        );
        self.save_project();
    }

    fn update_project_settings(&mut self, width: u32, height: u32, fps: f64, sample_rate: u32) {
        let Some(project) = self.project_mut() else {
            return;
        };
        project.timeline.width = width.max(16);
        project.timeline.height = height.max(16);
        project.timeline.fps = fps_from_float(fps);
        project.timeline.sample_rate = sample_rate.max(8_000);
        self.save_project();
    }

    fn render_command(&mut self) {
        let Some(project) = self.project() else {
            return;
        };
        let output = PathBuf::from(self.render_form.output_path.trim());
        match build_ffmpeg_command(
            project,
            &output,
            RenderSettings::from_timeline(&project.timeline),
        ) {
            Ok(command) => {
                self.render_form.last_command = command.display_shell();
                self.status = "Render command built".to_string();
            }
            Err(error) => {
                self.status = format!("Render command failed: {error}");
            }
        }
    }

    fn render_now(&mut self) {
        let Some(project) = self.project().cloned() else {
            return;
        };
        let output = PathBuf::from(self.render_form.output_path.trim());
        self.render_form.is_rendering = true;
        self.status = format!("Rendering {}", output.display());
        match build_ffmpeg_command(
            &project,
            &output,
            RenderSettings::from_timeline(&project.timeline),
        )
        .and_then(|command| command.spawn_and_wait())
        {
            Ok(()) => {
                self.render_form.is_rendering = false;
                self.status = format!("Rendered {}", output.display());
            }
            Err(error) => {
                self.render_form.is_rendering = false;
                self.status = format!("Render failed: {error}");
            }
        }
    }
}

impl eframe::App for StudioApp {
    fn logic(&mut self, context: &Context, _frame: &mut eframe::Frame) {
        self.maybe_reload_from_disk();
        context.request_repaint_after(Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        Panel::top("top_bar").show(ui, |ui| self.top_bar(ui));
        Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        match self.mode {
            WorkspaceMode::Home => self.home_ui(ui),
            WorkspaceMode::Editor => self.editor_ui(ui),
        }
    }
}

impl StudioApp {
    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new(APP_TITLE).strong());
            ui.separator();
            if ui.button("Home").clicked() {
                self.mode = WorkspaceMode::Home;
            }
            if ui
                .add_enabled(self.project.is_some(), Button::new("Save"))
                .clicked()
            {
                self.save_project();
            }
            if ui
                .add_enabled(self.project.is_some(), Button::new("Render"))
                .clicked()
            {
                self.inspector_tab = InspectorTab::Render;
                self.mode = WorkspaceMode::Editor;
            }
            if ui
                .add_enabled(self.project.is_some(), Button::new("Start MCP"))
                .clicked()
            {
                self.start_mcp();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(project) = self.project() {
                    ui.label(format!(
                        "{} x {}  {} fps",
                        project.timeline.width,
                        project.timeline.height,
                        project.timeline.fps.expression()
                    ));
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.status).color(Color32::LIGHT_GRAY));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mcp = if self.mcp_started {
                    format!("MCP http://127.0.0.1:{}/mcp", self.mcp_port)
                } else {
                    "MCP stopped".to_string()
                };
                ui.label(mcp);
            });
        });
    }

    fn home_ui(&mut self, ui: &mut Ui) {
        CentralPanel::default()
            .frame(Frame::default().fill(Color32::from_rgb(14, 16, 20)))
            .show(ui, |ui| {
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        ui.heading(RichText::new("Create Project").size(28.0));
                        ui.add_space(12.0);
                        Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_width(520.0);
                            labeled_text(ui, "Name", &mut self.new_project.name);
                            ui.horizontal(|ui| {
                                ui.label("Location");
                                ui.text_edit_singleline(&mut self.new_project.path);
                                if ui.button("Choose").clicked() {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_file_name("termfx.project.json")
                                        .save_file()
                                    {
                                        self.new_project.path = path.display().to_string();
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Resolution");
                                ui.add(
                                    DragValue::new(&mut self.new_project.width).range(16..=8192),
                                );
                                ui.label("x");
                                ui.add(
                                    DragValue::new(&mut self.new_project.height).range(16..=8192),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("FPS");
                                ui.add(
                                    DragValue::new(&mut self.new_project.fps)
                                        .speed(0.1)
                                        .range(1.0..=240.0),
                                );
                                ui.label("Sample rate");
                                ui.add(
                                    DragValue::new(&mut self.new_project.sample_rate)
                                        .range(8_000..=192_000),
                                );
                            });
                            if ui
                                .add_sized([180.0, 34.0], Button::new("Create and open"))
                                .clicked()
                            {
                                self.create_project();
                            }
                        });
                    });

                    ui.add_space(48.0);
                    ui.vertical(|ui| {
                        ui.heading(RichText::new("Open Project").size(28.0));
                        ui.add_space(12.0);
                        if ui
                            .add_sized([180.0, 34.0], Button::new("Open project file"))
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("TermFX project", &["json"])
                                .pick_file()
                            {
                                self.load_project(path);
                            }
                        }
                        ui.add_space(18.0);
                        ui.heading("Recent");
                        let recents = self.config.recent_projects.clone();
                        ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                            for path in recents {
                                let exists = path.exists();
                                let label = if exists {
                                    path.display().to_string()
                                } else {
                                    format!("{}  (missing)", path.display())
                                };
                                if ui.add_enabled(exists, Button::new(label)).clicked() {
                                    self.load_project(path);
                                }
                            }
                        });
                    });
                });
            });
    }

    fn editor_ui(&mut self, ui: &mut Ui) {
        Panel::left("media_panel")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| self.media_panel(ui));
        Panel::right("inspector_panel")
            .resizable(true)
            .default_size(380.0)
            .show(ui, |ui| self.inspector_panel(ui));
        Panel::bottom("timeline_panel")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| self.timeline_panel(ui));
        CentralPanel::default().show(ui, |ui| self.viewer_panel(ui));
    }

    fn media_panel(&mut self, ui: &mut Ui) {
        ui.heading("Media");
        ui.separator();
        if ui.button("Import media").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                self.import_form.path = path.display().to_string();
                self.import_form.name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("media")
                    .to_string();
            }
        }
        labeled_text(ui, "Path", &mut self.import_form.path);
        labeled_text(ui, "Name", &mut self.import_form.name);
        ui.horizontal(|ui| {
            ui.label("Kind");
            ComboBox::from_id_salt("asset_kind")
                .selected_text(format!("{:?}", self.import_form.kind))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.import_form.kind, AssetKind::Video, "Video");
                    ui.selectable_value(&mut self.import_form.kind, AssetKind::Audio, "Audio");
                    ui.selectable_value(&mut self.import_form.kind, AssetKind::Image, "Image");
                });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.import_form.append_to_timeline, "Append");
            ui.label("Track");
            ui.add(DragValue::new(&mut self.import_form.target_track).range(0..=64));
        });
        ui.horizontal(|ui| {
            ui.label("Duration");
            ui.add(
                DragValue::new(&mut self.import_form.duration_seconds)
                    .speed(0.1)
                    .range(0.1..=86_400.0),
            );
            ui.label("sec");
        });
        if ui.button("Add to project").clicked() {
            self.import_media();
        }
        if ui.button("Append selected asset").clicked() {
            self.append_selected_asset();
        }
        if ui.button("Add text clip").clicked() {
            self.add_text_clip();
        }

        ui.separator();
        if let Some(project) = self.project() {
            let assets = project.media.clone();
            ScrollArea::vertical().show(ui, |ui| {
                for asset in assets {
                    self.asset_row(ui, &asset);
                }
            });
        }
    }

    fn asset_row(&mut self, ui: &mut Ui, asset: &MediaAsset) {
        let selected = self.selected_asset == Some(asset.id);
        let label = format!("{}  {:?}", asset.name, asset.kind);
        if ui.selectable_label(selected, label).clicked() {
            self.selected_asset = Some(asset.id);
            self.import_form.path = asset.path.display().to_string();
            self.import_form.name = asset.name.clone();
        }
        ui.small(asset.path.display().to_string());
    }

    fn viewer_panel(&mut self, ui: &mut Ui) {
        let Some(project) = self.project().cloned() else {
            ui.centered_and_justified(|ui| {
                ui.heading("Open or create a project");
            });
            return;
        };

        let timeline = project.timeline.clone();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Viewer");
                ui.separator();
                ui.label(format!(
                    "Playhead {:.2}s",
                    timeline.fps.seconds_from_frames(self.playhead_frame)
                ));
                if ui.button("Open render").clicked() {
                    open_path(Path::new(self.render_form.output_path.trim()));
                }
            });
            let available = ui.available_size();
            let viewer_size = Vec2::new(available.x.max(320.0), (available.y - 150.0).max(240.0));
            let (rect, _) = ui.allocate_exact_size(viewer_size, Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, Color32::from_rgb(8, 9, 12));
            let target_aspect = timeline.width as f32 / timeline.height.max(1) as f32;
            let mut frame_w = rect.width() * 0.86;
            let mut frame_h = frame_w / target_aspect;
            if frame_h > rect.height() * 0.86 {
                frame_h = rect.height() * 0.86;
                frame_w = frame_h * target_aspect;
            }
            let frame_rect =
                egui::Rect::from_center_size(rect.center(), Vec2::new(frame_w, frame_h));
            painter.rect_filled(frame_rect, 2.0, Color32::from_rgb(22, 24, 28));
            painter.rect_stroke(
                frame_rect,
                2.0,
                Stroke::new(1.0, Color32::from_rgb(70, 74, 84)),
                egui::StrokeKind::Inside,
            );

            if let Some(clip) = self.selected_clip() {
                painter.text(
                    frame_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "{}\n{} effects  {} keyframes",
                        clip.name,
                        clip.effects.len(),
                        clip.keyframes.len()
                    ),
                    FontId::proportional(24.0),
                    Color32::WHITE,
                );
            } else {
                painter.text(
                    frame_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No clip selected",
                    FontId::proportional(24.0),
                    Color32::GRAY,
                );
            }

            ui.add_space(8.0);
            self.transport_controls(ui);
        });
    }

    fn transport_controls(&mut self, ui: &mut Ui) {
        let duration = self
            .project()
            .map(|project| project.timeline.duration_frames())
            .unwrap_or(0);
        ui.horizontal(|ui| {
            if ui.button("|<").clicked() {
                self.playhead_frame = 0;
            }
            if ui.button("<").clicked() {
                self.playhead_frame = self.playhead_frame.saturating_sub(1);
            }
            if ui.button(">").clicked() {
                self.playhead_frame = (self.playhead_frame + 1).min(duration);
            }
            if ui.button(">|").clicked() {
                self.playhead_frame = duration;
            }
            ui.label("Frame");
            ui.add(DragValue::new(&mut self.playhead_frame).range(0..=duration.max(1)));
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut self.zoom, 0.25..=8.0));
        });
    }

    fn timeline_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Timeline");
            if ui.button("Split").clicked() {
                self.split_selected_clip();
            }
            if ui.button("Delete").clicked() {
                self.delete_selected_clip();
            }
            if ui.button("Marker").clicked() {
                self.add_marker_at_playhead();
            }
            if ui.button("Keyframe").clicked() {
                self.add_keyframe_at_playhead();
            }
        });
        ui.separator();

        let Some(project) = self.project().cloned() else {
            return;
        };
        let timeline = project.timeline;
        let duration = timeline
            .duration_frames()
            .max(timeline.fps.frames_from_seconds(10.0));
        let pixels_per_frame = (ui.available_width() / duration as f32 * self.zoom).max(0.5);
        ScrollArea::both().show(ui, |ui| {
            for track in &timeline.tracks {
                ui.horizontal(|ui| {
                    ui.set_height(48.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(90.0, 42.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.label(format!("{} {:?}", track.name, track.kind));
                        },
                    );
                    let width = (duration as f32 * pixels_per_frame).max(ui.available_width());
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::click());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 3.0, Color32::from_rgb(26, 28, 34));
                    for marker in &timeline.markers {
                        let x = rect.left() + marker.frame as f32 * pixels_per_frame;
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            Stroke::new(1.0, Color32::from_rgb(80, 220, 220)),
                        );
                    }
                    for clip in &track.clips {
                        let x = rect.left() + clip.start_frame as f32 * pixels_per_frame;
                        let w = (clip.duration_frames as f32 * pixels_per_frame).max(36.0);
                        let clip_rect = egui::Rect::from_min_size(
                            egui::pos2(x, rect.top() + 5.0),
                            Vec2::new(w, 32.0),
                        );
                        let selected = self.selected_clip == Some(clip.id);
                        let color = if selected {
                            Color32::from_rgb(68, 132, 255)
                        } else if track.kind == TrackKind::Audio {
                            Color32::from_rgb(72, 150, 112)
                        } else {
                            Color32::from_rgb(92, 96, 116)
                        };
                        painter.rect_filled(clip_rect, 4.0, color);
                        painter.text(
                            clip_rect.left_center() + Vec2::new(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &clip.name,
                            FontId::proportional(13.0),
                            Color32::WHITE,
                        );
                    }
                    let playhead_x = rect.left() + self.playhead_frame as f32 * pixels_per_frame;
                    painter.line_segment(
                        [
                            egui::pos2(playhead_x, rect.top()),
                            egui::pos2(playhead_x, rect.bottom()),
                        ],
                        Stroke::new(2.0, Color32::from_rgb(255, 224, 100)),
                    );
                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            self.playhead_frame = ((pos.x - rect.left()) / pixels_per_frame)
                                .max(0.0)
                                as TimelineFrame;
                            self.selected_clip =
                                hit_test_clip(track, self.playhead_frame).or(self.selected_clip);
                        }
                    }
                });
            }
        });
    }

    fn inspector_panel(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            tab_button(ui, &mut self.inspector_tab, InspectorTab::Clip, "Clip");
            tab_button(
                ui,
                &mut self.inspector_tab,
                InspectorTab::Effects,
                "Effects",
            );
            tab_button(
                ui,
                &mut self.inspector_tab,
                InspectorTab::Keyframes,
                "Keyframes",
            );
            tab_button(
                ui,
                &mut self.inspector_tab,
                InspectorTab::Project,
                "Project",
            );
            tab_button(ui, &mut self.inspector_tab, InspectorTab::Render, "Render");
            tab_button(ui, &mut self.inspector_tab, InspectorTab::Automation, "MCP");
        });
        ui.separator();
        match self.inspector_tab {
            InspectorTab::Clip => self.clip_inspector(ui),
            InspectorTab::Effects => self.effects_inspector(ui),
            InspectorTab::Keyframes => self.keyframes_inspector(ui),
            InspectorTab::Project => self.project_inspector(ui),
            InspectorTab::Render => self.render_inspector(ui),
            InspectorTab::Automation => self.automation_inspector(ui),
        }
    }

    fn clip_inspector(&mut self, ui: &mut Ui) {
        let fps = self
            .project()
            .map(|project| project.timeline.fps)
            .unwrap_or_default();
        let mut changed = false;
        if let Some(clip) = self.selected_clip_mut() {
            ui.heading(&clip.name);
            changed |= labeled_text(ui, "Name", &mut clip.name);
            let mut start = fps.seconds_from_frames(clip.start_frame);
            let mut duration = fps.seconds_from_frames(clip.duration_frames);
            ui.horizontal(|ui| {
                ui.label("Start");
                changed |= ui
                    .add(DragValue::new(&mut start).speed(0.05).range(0.0..=86_400.0))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Duration");
                changed |= ui
                    .add(
                        DragValue::new(&mut duration)
                            .speed(0.05)
                            .range(0.01..=86_400.0),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Opacity");
                changed |= ui
                    .add(egui::Slider::new(&mut clip.opacity, 0.0..=1.0))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Volume");
                changed |= ui
                    .add(egui::Slider::new(&mut clip.volume, 0.0..=2.0))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Speed");
                changed |= ui
                    .add(egui::Slider::new(&mut clip.speed, 0.05..=8.0))
                    .changed();
            });
            if changed {
                clip.start_frame = fps.frames_from_seconds(start);
                clip.duration_frames = fps.frames_from_seconds(duration).max(1);
            }
        } else {
            ui.label("Select a clip on the timeline.");
        }
        if changed {
            self.save_project();
        }
    }

    fn effects_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Effect Library");
        ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
            let by_category = effect_library_by_category();
            for (category, specs) in by_category {
                ui.collapsing(category, |ui| {
                    for spec in specs {
                        ui.horizontal(|ui| {
                            ui.label(spec.name);
                            ui.small(spec.description);
                        });
                    }
                });
            }
        });
        ui.separator();
        ui.heading("Add Effect");
        for preset in EFFECT_PRESETS {
            if ui.button(preset.label()).clicked() {
                self.add_effect_to_selected_clip(*preset);
            }
        }
        ui.separator();
        ui.heading("Selected Clip Stack");
        if let Some(clip) = self.selected_clip().cloned() {
            for effect in clip.effects {
                let selected = self.selected_effect == Some(effect.id);
                ui.horizontal(|ui| {
                    if ui.selectable_label(selected, &effect.name).clicked() {
                        self.selected_effect = Some(effect.id);
                    }
                    ui.label(if effect.enabled { "on" } else { "off" });
                });
            }
            if ui.button("Remove selected effect").clicked() {
                self.remove_selected_effect();
            }
        }
    }

    fn keyframes_inspector(&mut self, ui: &mut Ui) {
        if ui.button("Add keyframe at playhead").clicked() {
            self.add_keyframe_at_playhead();
        }
        ui.separator();
        let Some(clip_id) = self.selected_clip else {
            ui.label("Select a clip first.");
            return;
        };
        let fps = self
            .project()
            .map(|project| project.timeline.fps)
            .unwrap_or_default();
        let mut changed = false;
        let mut selected_keyframe = self.selected_keyframe;
        if let Some(clip) = self
            .project_mut()
            .and_then(|project| project.timeline.clip_mut(clip_id).ok())
        {
            for keyframe in &mut clip.keyframes {
                ui.group(|ui| {
                    let selected = selected_keyframe == Some(keyframe.id);
                    if ui
                        .selectable_label(
                            selected,
                            format!("{:.2}s", fps.seconds_from_frames(keyframe.frame)),
                        )
                        .clicked()
                    {
                        selected_keyframe = Some(keyframe.id);
                    }
                    changed |= keyframe_editor(ui, keyframe, fps);
                });
            }
        }
        self.selected_keyframe = selected_keyframe;
        if changed {
            self.save_project();
        }
    }

    fn project_inspector(&mut self, ui: &mut Ui) {
        let Some(project) = self.project().cloned() else {
            return;
        };
        let mut width = project.timeline.width;
        let mut height = project.timeline.height;
        let mut fps =
            project.timeline.fps.numerator as f64 / project.timeline.fps.denominator as f64;
        let mut sample_rate = project.timeline.sample_rate;
        ui.heading("Project Settings");
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Width");
            changed |= ui
                .add(DragValue::new(&mut width).range(16..=8192))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Height");
            changed |= ui
                .add(DragValue::new(&mut height).range(16..=8192))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("FPS");
            changed |= ui
                .add(DragValue::new(&mut fps).speed(0.1).range(1.0..=240.0))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Sample rate");
            changed |= ui
                .add(DragValue::new(&mut sample_rate).range(8_000..=192_000))
                .changed();
        });
        if changed {
            self.update_project_settings(width, height, fps, sample_rate);
        }
        ui.separator();
        ui.label(format!("Tracks: {}", project.timeline.tracks.len()));
        ui.label(format!("Assets: {}", project.media.len()));
        ui.label(format!("Markers: {}", project.timeline.markers.len()));
        if ui.button("Add video track").clicked() {
            if let Some(project) = self.project_mut() {
                project.timeline.add_track(TrackKind::Video, None);
                self.save_project();
            }
        }
        if ui.button("Add audio track").clicked() {
            if let Some(project) = self.project_mut() {
                project.timeline.add_track(TrackKind::Audio, None);
                self.save_project();
            }
        }
    }

    fn render_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Render");
        ui.horizontal(|ui| {
            ui.label("Output");
            ui.text_edit_singleline(&mut self.render_form.output_path);
            if ui.button("Choose").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("termfx_render.mp4")
                    .save_file()
                {
                    self.render_form.output_path = path.display().to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Build command").clicked() {
                self.render_command();
            }
            if ui
                .add_enabled(!self.render_form.is_rendering, Button::new("Render now"))
                .clicked()
            {
                self.render_now();
            }
            if ui.button("Open output").clicked() {
                open_path(Path::new(self.render_form.output_path.trim()));
            }
        });
        ui.separator();
        ui.label("Command");
        ui.add(
            TextEdit::multiline(&mut self.render_form.last_command)
                .desired_rows(12)
                .code_editor(),
        );
    }

    fn automation_inspector(&mut self, ui: &mut Ui) {
        ui.heading("MCP");
        ui.checkbox(&mut self.mcp_enabled, "Start with project");
        ui.horizontal(|ui| {
            ui.label("Port");
            ui.add(DragValue::new(&mut self.mcp_port).range(1024..=65_535));
        });
        if ui.button("Start MCP server").clicked() {
            self.start_mcp();
        }
        ui.label(if self.mcp_started {
            format!("http://127.0.0.1:{}/mcp", self.mcp_port)
        } else {
            "Server stopped".to_string()
        });
        ui.separator();
        ui.heading("Activity");
        ScrollArea::vertical().show(ui, |ui| {
            for event in &self.mcp_events {
                ui.label(event);
            }
        });
    }
}

fn configure_style(_context: &Context) {
    // The UI uses explicit frames and colors where contrast matters.
}

fn labeled_text(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui.text_edit_singleline(value).changed();
    });
    changed
}

fn tab_button(ui: &mut Ui, selected: &mut InspectorTab, tab: InspectorTab, label: &str) {
    if ui.selectable_label(*selected == tab, label).clicked() {
        *selected = tab;
    }
}

fn keyframe_editor(ui: &mut Ui, keyframe: &mut TransformKeyframe, fps: Fps) -> bool {
    let mut changed = false;
    let mut seconds = fps.seconds_from_frames(keyframe.frame);
    ui.horizontal(|ui| {
        ui.label("Time");
        changed |= ui
            .add(
                DragValue::new(&mut seconds)
                    .speed(0.05)
                    .range(0.0..=86_400.0),
            )
            .changed();
        ComboBox::from_id_salt(("easing", keyframe.id))
            .selected_text(format!("{:?}", keyframe.easing))
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(&mut keyframe.easing, KeyframeEasing::Hold, "Hold")
                    .changed();
                changed |= ui
                    .selectable_value(&mut keyframe.easing, KeyframeEasing::Linear, "Linear")
                    .changed();
                changed |= ui
                    .selectable_value(&mut keyframe.easing, KeyframeEasing::EaseIn, "Ease in")
                    .changed();
                changed |= ui
                    .selectable_value(&mut keyframe.easing, KeyframeEasing::EaseOut, "Ease out")
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut keyframe.easing,
                        KeyframeEasing::EaseInOut,
                        "Ease in/out",
                    )
                    .changed();
            });
    });
    egui::Grid::new(("keyframe_grid", keyframe.id))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("X");
            changed |= ui.add(DragValue::new(&mut keyframe.x).speed(1.0)).changed();
            ui.end_row();
            ui.label("Y");
            changed |= ui.add(DragValue::new(&mut keyframe.y).speed(1.0)).changed();
            ui.end_row();
            ui.label("Scale");
            changed |= ui
                .add(
                    DragValue::new(&mut keyframe.scale)
                        .speed(0.01)
                        .range(0.01..=20.0),
                )
                .changed();
            ui.end_row();
            ui.label("Rotation");
            changed |= ui
                .add(DragValue::new(&mut keyframe.rotation_degrees).speed(0.5))
                .changed();
            ui.end_row();
            ui.label("Opacity");
            changed |= ui
                .add(egui::Slider::new(&mut keyframe.opacity, 0.0..=1.0))
                .changed();
            ui.end_row();
            ui.label("Volume");
            changed |= ui
                .add(egui::Slider::new(&mut keyframe.volume, 0.0..=2.0))
                .changed();
            ui.end_row();
        });
    if changed {
        keyframe.frame = fps.frames_from_seconds(seconds);
    }
    changed
}

fn effect_library_by_category() -> BTreeMap<&'static str, Vec<crate::core::effect::EffectSpec>> {
    let mut categories = BTreeMap::new();
    for spec in effect_library() {
        categories
            .entry(spec.category)
            .or_insert_with(Vec::new)
            .push(spec);
    }
    categories
}

fn hit_test_clip(track: &crate::core::timeline::Track, frame: TimelineFrame) -> Option<Uuid> {
    track
        .clips
        .iter()
        .find(|clip| frame >= clip.start_frame && frame <= clip.end_frame())
        .map(|clip| clip.id)
}

fn next_track_start(timeline: &Timeline, track_index: usize) -> TimelineFrame {
    timeline
        .tracks
        .iter()
        .find(|track| track.index == track_index)
        .and_then(|track| track.clips.iter().map(Clip::end_frame).max())
        .unwrap_or(0)
}

fn first_clip_id(project: Option<&Project>) -> Option<Uuid> {
    project?
        .timeline
        .tracks
        .iter()
        .flat_map(|track| track.clips.iter())
        .next()
        .map(|clip| clip.id)
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn fps_from_float(fps: f64) -> Fps {
    if !fps.is_finite() || fps <= 0.0 {
        return Fps::broadcast();
    }
    Fps::new((fps * 1_000.0).round() as u32, 1_000)
}

fn config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".termfx")
        .join(CONFIG_FILE)
}

fn load_config(path: &Path) -> StudioConfig {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_config(path: &Path, config: &StudioConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(config)?)
}

fn read_mcp_events(path: &Path, limit: usize) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return vec!["No activity yet.".to_string()];
    };
    let mut events = contents
        .lines()
        .rev()
        .filter_map(format_mcp_event)
        .take(limit)
        .collect::<Vec<_>>();
    events.reverse();
    if events.is_empty() {
        vec!["No activity yet.".to_string()]
    } else {
        events
    }
}

fn format_mcp_event(line: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    let tool = value.get("tool").and_then(Value::as_str).unwrap_or("tool");
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("status");
    let arguments = value
        .get("arguments")
        .filter(|value| !value.is_null())
        .map(Value::to_string)
        .unwrap_or_default();
    if arguments.is_empty() {
        Some(format!("{status} {tool}"))
    } else {
        Some(format!("{status} {tool} {arguments}"))
    }
}

fn open_path(path: &Path) {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start"]).arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    let _ = command.spawn();
}
