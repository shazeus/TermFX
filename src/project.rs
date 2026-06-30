use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::effect::{Effect, EffectInstance};
use crate::core::media::{AssetKind, MediaAsset};
use crate::core::time::Frame;
use crate::core::timeline::{Clip, Timeline, TrackKind};
use crate::error::Result;

const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    pub schema_version: u32,
    pub name: String,
    pub root: PathBuf,
    pub media: Vec<MediaAsset>,
    pub timeline: Timeline,
    pub metadata: HashMap<String, String>,
}

impl Project {
    pub fn new(name: impl Into<String>, root: PathBuf) -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            name: name.into(),
            root,
            media: Vec::new(),
            timeline: Timeline::default(),
            metadata: HashMap::new(),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn add_media(
        &mut self,
        path: PathBuf,
        kind: AssetKind,
        name: Option<String>,
    ) -> MediaAsset {
        let asset = MediaAsset::new(path, kind, name);
        self.media.push(asset.clone());
        asset
    }

    pub fn add_media_clip(
        &mut self,
        media_id: Uuid,
        track_index: usize,
        start_frame: Frame,
        duration_frames: Frame,
    ) -> Result<Uuid> {
        let asset_name = self
            .media
            .iter()
            .find(|asset| asset.id == media_id)
            .map(|asset| asset.name.clone())
            .ok_or(crate::TermFxError::MissingMedia(media_id))?;
        let track_kind = self
            .timeline
            .tracks
            .iter()
            .find(|track| track.index == track_index)
            .map(|track| track.kind)
            .ok_or(crate::TermFxError::MissingTrack(track_index))?;
        let clip = Clip::media(
            asset_name,
            media_id,
            track_kind,
            start_frame,
            duration_frames,
        );
        self.timeline.add_clip(track_index, clip)
    }

    pub fn add_text_clip(
        &mut self,
        track_index: usize,
        text: String,
        start_frame: Frame,
        duration_frames: Frame,
    ) -> Result<Uuid> {
        let clip = Clip::text("text", text, start_frame, duration_frames);
        self.timeline.add_clip(track_index, clip)
    }

    pub fn apply_effect(&mut self, clip_id: Uuid, effect: Effect) -> Result<Uuid> {
        let effect_name = match &effect {
            Effect::BlackWhite => "black and white",
            Effect::Glitch { .. } => "glitch",
            Effect::FadeIn { .. } => "fade in",
            Effect::FadeOut { .. } => "fade out",
            Effect::SShake { .. } => "s_shake",
            Effect::TextOverlay { .. } => "text overlay",
        };
        let instance = EffectInstance::new(effect_name, effect);
        let id = instance.id;
        self.timeline.clip_mut(clip_id)?.effects.push(instance);
        Ok(id)
    }

    pub fn video_track_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.timeline
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Video)
            .map(|track| track.index)
    }
}
