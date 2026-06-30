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
    Sepia,
    Invert,
    EdgeDetect,
    Glitch {
        intensity: f32,
    },
    BrightnessContrast {
        brightness: f32,
        contrast: f32,
        saturation: f32,
    },
    HueRotate {
        degrees: f32,
    },
    GaussianBlur {
        sigma: f32,
    },
    BoxBlur {
        radius: u32,
    },
    Sharpen {
        amount: f32,
    },
    Vignette {
        angle: f32,
    },
    FilmGrain {
        strength: f32,
    },
    Pixelate {
        block_size: u32,
    },
    ChromaticAberration {
        offset_px: i32,
    },
    LensDistortion {
        k1: f32,
        k2: f32,
    },
    Posterize {
        levels: u8,
    },
    Letterbox {
        height_px: u32,
        color: String,
    },
    Border {
        thickness_px: u32,
        color: String,
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

impl Effect {
    pub fn kind(&self) -> &'static str {
        match self {
            Effect::BlackWhite => "black_and_white",
            Effect::Sepia => "sepia",
            Effect::Invert => "invert",
            Effect::EdgeDetect => "edge_detect",
            Effect::Glitch { .. } => "glitch",
            Effect::BrightnessContrast { .. } => "brightness_contrast",
            Effect::HueRotate { .. } => "hue_rotate",
            Effect::GaussianBlur { .. } => "gaussian_blur",
            Effect::BoxBlur { .. } => "box_blur",
            Effect::Sharpen { .. } => "sharpen",
            Effect::Vignette { .. } => "vignette",
            Effect::FilmGrain { .. } => "film_grain",
            Effect::Pixelate { .. } => "pixelate",
            Effect::ChromaticAberration { .. } => "chromatic_aberration",
            Effect::LensDistortion { .. } => "lens_distortion",
            Effect::Posterize { .. } => "posterize",
            Effect::Letterbox { .. } => "letterbox",
            Effect::Border { .. } => "border",
            Effect::FadeIn { .. } => "fade_in",
            Effect::FadeOut { .. } => "fade_out",
            Effect::SShake { .. } => "s_shake",
            Effect::TextOverlay { .. } => "text_overlay",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EffectSpec {
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
}

pub fn effect_library() -> Vec<EffectSpec> {
    vec![
        EffectSpec {
            name: "black_and_white",
            category: "color",
            description: "Desaturates the clip.",
        },
        EffectSpec {
            name: "sepia",
            category: "color",
            description: "Applies a warm sepia tone.",
        },
        EffectSpec {
            name: "invert",
            category: "color",
            description: "Inverts luma and chroma values.",
        },
        EffectSpec {
            name: "brightness_contrast",
            category: "color",
            description: "Adjusts brightness, contrast, and saturation.",
        },
        EffectSpec {
            name: "hue_rotate",
            category: "color",
            description: "Rotates hue by degrees.",
        },
        EffectSpec {
            name: "glitch",
            category: "stylize",
            description: "Adds RGB channel offset and temporal noise.",
        },
        EffectSpec {
            name: "film_grain",
            category: "stylize",
            description: "Adds procedural film grain.",
        },
        EffectSpec {
            name: "pixelate",
            category: "stylize",
            description: "Applies blocky nearest-neighbor pixelation.",
        },
        EffectSpec {
            name: "posterize",
            category: "stylize",
            description: "Reduces RGB tonal levels.",
        },
        EffectSpec {
            name: "edge_detect",
            category: "stylize",
            description: "Highlights high-contrast edges.",
        },
        EffectSpec {
            name: "gaussian_blur",
            category: "blur",
            description: "Applies Gaussian blur.",
        },
        EffectSpec {
            name: "box_blur",
            category: "blur",
            description: "Applies box blur.",
        },
        EffectSpec {
            name: "sharpen",
            category: "detail",
            description: "Sharpens image detail with unsharp mask.",
        },
        EffectSpec {
            name: "vignette",
            category: "lens",
            description: "Darkens the frame edges.",
        },
        EffectSpec {
            name: "chromatic_aberration",
            category: "lens",
            description: "Offsets RGB channels for lens fringing.",
        },
        EffectSpec {
            name: "lens_distortion",
            category: "lens",
            description: "Applies barrel or pincushion distortion.",
        },
        EffectSpec {
            name: "letterbox",
            category: "layout",
            description: "Draws top and bottom cinematic bars.",
        },
        EffectSpec {
            name: "border",
            category: "layout",
            description: "Draws a border around the clip frame.",
        },
        EffectSpec {
            name: "fade_in",
            category: "transition",
            description: "Fades clip alpha in.",
        },
        EffectSpec {
            name: "fade_out",
            category: "transition",
            description: "Fades clip alpha out.",
        },
        EffectSpec {
            name: "s_shake",
            category: "motion",
            description: "After Effects style procedural shake.",
        },
        EffectSpec {
            name: "text_overlay",
            category: "text",
            description: "Draws text over the clip.",
        },
    ]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransformKeyframe {
    #[serde(default = "new_uuid")]
    pub id: Uuid,
    pub frame: Frame,
    pub x: f32,
    pub y: f32,
    #[serde(default = "default_one")]
    pub scale: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default = "default_one")]
    pub opacity: f32,
    #[serde(default = "default_one")]
    pub volume: f32,
    #[serde(default)]
    pub easing: KeyframeEasing,
}

impl TransformKeyframe {
    pub fn new(frame: Frame) -> Self {
        Self {
            id: Uuid::new_v4(),
            frame,
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation_degrees: 0.0,
            opacity: 1.0,
            volume: 1.0,
            easing: KeyframeEasing::Linear,
        }
    }

    pub fn value(&self, property: KeyframeProperty) -> f32 {
        match property {
            KeyframeProperty::X => self.x,
            KeyframeProperty::Y => self.y,
            KeyframeProperty::Scale => self.scale,
            KeyframeProperty::RotationDegrees => self.rotation_degrees,
            KeyframeProperty::Opacity => self.opacity,
            KeyframeProperty::Volume => self.volume,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyframeEasing {
    Hold,
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyframeProperty {
    X,
    Y,
    Scale,
    RotationDegrees,
    Opacity,
    Volume,
}

impl KeyframeProperty {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "scale" => Some(Self::Scale),
            "rotation_degrees" | "rotation" => Some(Self::RotationDegrees),
            "opacity" => Some(Self::Opacity),
            "volume" => Some(Self::Volume),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            KeyframeProperty::X => "x",
            KeyframeProperty::Y => "y",
            KeyframeProperty::Scale => "scale",
            KeyframeProperty::RotationDegrees => "rotation_degrees",
            KeyframeProperty::Opacity => "opacity",
            KeyframeProperty::Volume => "volume",
        }
    }
}

fn new_uuid() -> Uuid {
    Uuid::new_v4()
}

fn default_one() -> f32 {
    1.0
}
