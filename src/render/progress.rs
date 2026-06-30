use serde::{Deserialize, Serialize};

use crate::core::time::Frame;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderProgress {
    pub frame: Frame,
    pub total_frames: Frame,
    pub fps: f32,
    pub speed: f32,
}

impl RenderProgress {
    pub fn percent(&self) -> f32 {
        if self.total_frames == 0 {
            0.0
        } else {
            (self.frame as f32 / self.total_frames as f32 * 100.0).clamp(0.0, 100.0)
        }
    }
}
