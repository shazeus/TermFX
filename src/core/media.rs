use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::time::Frame;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Video,
    Audio,
    Image,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MediaAsset {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub kind: AssetKind,
    pub duration_frames: Option<Frame>,
    pub has_audio: bool,
}

impl MediaAsset {
    pub fn new(path: PathBuf, kind: AssetKind, name: Option<String>) -> Self {
        let fallback_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("media")
            .to_string();
        let has_audio = matches!(kind, AssetKind::Video | AssetKind::Audio);

        Self {
            id: Uuid::new_v4(),
            name: name.unwrap_or(fallback_name),
            path,
            kind,
            duration_frames: None,
            has_audio,
        }
    }
}
