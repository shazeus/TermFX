use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::time::Frame;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EffectInstance {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub effect: Effect,
}

impl EffectInstance {
    pub fn new(name: impl Into<String>, effect: Effect) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: true,
            effect,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Effect {
    BlackWhite,
    Glitch {
        intensity: f32,
    },
    FadeIn {
        duration_frames: Frame,
    },
    FadeOut {
        duration_frames: Frame,
    },
    SShake {
        amplitude_px: u32,
        frequency_hz: f32,
        seed: f32,
    },
    TextOverlay {
        text: String,
        x: i32,
        y: i32,
        font_size: u32,
        color: String,
        start_frame: Frame,
        duration_frames: Frame,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransformKeyframe {
    pub frame: Frame,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation_degrees: f32,
    pub opacity: f32,
}
