use crate::core::time::{Fps, Frame};

pub fn seconds(fps: Fps, frame: Frame) -> f64 {
    fps.seconds_from_frames(frame)
}

pub fn escape_drawtext(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
        .replace('%', "\\%")
        .replace('\n', "\\n")
}
