use serde::{Deserialize, Serialize};

pub type Frame = u64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Fps {
    pub numerator: u32,
    pub denominator: u32,
}

impl Fps {
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub const fn film() -> Self {
        Self::new(24_000, 1_001)
    }

    pub const fn ntsc() -> Self {
        Self::new(30_000, 1_001)
    }

    pub const fn broadcast() -> Self {
        Self::new(30, 1)
    }

    pub fn frames_from_seconds(self, seconds: f64) -> Frame {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }

        ((seconds * self.numerator as f64) / self.denominator as f64).round() as Frame
    }

    pub fn seconds_from_frames(self, frames: Frame) -> f64 {
        frames as f64 * self.denominator as f64 / self.numerator as f64
    }

    pub fn expression(self) -> String {
        if self.denominator == 1 {
            self.numerator.to_string()
        } else {
            format!("{}/{}", self.numerator, self.denominator)
        }
    }
}

impl Default for Fps {
    fn default() -> Self {
        Self::broadcast()
    }
}
