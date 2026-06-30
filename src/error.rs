use thiserror::Error;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, TermFxError>;

#[derive(Debug, Error)]
pub enum TermFxError {
    #[error("project I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("project JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing media asset: {0}")]
    MissingMedia(Uuid),
    #[error("missing clip: {0}")]
    MissingClip(Uuid),
    #[error("invalid time range: start frame {start} must be before end frame {end}")]
    InvalidRange { start: u64, end: u64 },
    #[error("track {0} does not exist")]
    MissingTrack(usize),
    #[error("track kind mismatch for track {track_index}")]
    TrackKindMismatch { track_index: usize },
    #[error("MCP request is invalid: {0}")]
    InvalidMcpRequest(String),
    #[error("FFmpeg command failed with status {0}")]
    FfmpegFailed(std::process::ExitStatus),
}
