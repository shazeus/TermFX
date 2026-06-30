use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, TermFxError};

use super::effect::EffectInstance;
use super::time::{Fps, Frame};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Audio,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Timeline {
    pub fps: Fps,
    pub width: u32,
    pub height: u32,
    pub sample_rate: u32,
    pub tracks: Vec<Track>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            fps: Fps::broadcast(),
            width: 1_920,
            height: 1_080,
            sample_rate: 48_000,
            tracks: vec![
                Track::new(0, TrackKind::Video, "V1"),
                Track::new(1, TrackKind::Video, "V2"),
                Track::new(2, TrackKind::Audio, "A1"),
            ],
        }
    }
}

impl Timeline {
    pub fn duration_frames(&self) -> Frame {
        self.tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(|clip| clip.end_frame())
            .max()
            .unwrap_or(0)
    }

    pub fn track_mut(&mut self, index: usize) -> Result<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|track| track.index == index)
            .ok_or(TermFxError::MissingTrack(index))
    }

    pub fn clip_mut(&mut self, id: Uuid) -> Result<&mut Clip> {
        self.tracks
            .iter_mut()
            .flat_map(|track| track.clips.iter_mut())
            .find(|clip| clip.id == id)
            .ok_or(TermFxError::MissingClip(id))
    }

    pub fn clip(&self, id: Uuid) -> Result<&Clip> {
        self.tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .find(|clip| clip.id == id)
            .ok_or(TermFxError::MissingClip(id))
    }

    pub fn add_clip(&mut self, track_index: usize, clip: Clip) -> Result<Uuid> {
        let track = self.track_mut(track_index)?;
        if track.kind != clip.track_kind {
            return Err(TermFxError::TrackKindMismatch { track_index });
        }

        let id = clip.id;
        track.clips.push(clip);
        track.clips.sort_by_key(|clip| (clip.start_frame, clip.id));
        Ok(id)
    }

    pub fn trim_clip_to_source_range(
        &mut self,
        clip_id: Uuid,
        source_start: Frame,
        source_end: Frame,
    ) -> Result<()> {
        if source_start >= source_end {
            return Err(TermFxError::InvalidRange {
                start: source_start,
                end: source_end,
            });
        }

        let clip = self.clip_mut(clip_id)?;
        clip.trim_start_frame = source_start;
        clip.duration_frames = source_end - source_start;
        Ok(())
    }

    pub fn remove_timeline_range(
        &mut self,
        start_frame: Frame,
        end_frame: Frame,
        ripple: bool,
    ) -> Result<()> {
        if start_frame >= end_frame {
            return Err(TermFxError::InvalidRange {
                start: start_frame,
                end: end_frame,
            });
        }

        let removed = end_frame - start_frame;
        for track in &mut self.tracks {
            let mut rewritten = Vec::with_capacity(track.clips.len());
            for clip in track.clips.drain(..) {
                let clip_start = clip.start_frame;
                let clip_end = clip.end_frame();

                if clip_end <= start_frame || clip_start >= end_frame {
                    rewritten.push(clip);
                    continue;
                }

                if clip_start < start_frame {
                    let mut left = clip.clone();
                    left.duration_frames = start_frame - clip_start;
                    rewritten.push(left);
                }

                if clip_end > end_frame {
                    let mut right = clip;
                    let consumed_source = end_frame.saturating_sub(clip_start);
                    right.id = Uuid::new_v4();
                    right.name = format!("{} (split)", right.name);
                    right.trim_start_frame += consumed_source;
                    right.duration_frames = clip_end - end_frame;
                    right.start_frame = end_frame;
                    rewritten.push(right);
                }
            }

            if ripple {
                for clip in &mut rewritten {
                    if clip.start_frame >= end_frame {
                        clip.start_frame -= removed;
                    }
                }
            }

            rewritten.sort_by_key(|clip| (clip.start_frame, clip.id));
            track.clips = rewritten;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Track {
    pub index: usize,
    pub kind: TrackKind,
    pub name: String,
    pub muted: bool,
    pub locked: bool,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(index: usize, kind: TrackKind, name: impl Into<String>) -> Self {
        Self {
            index,
            kind,
            name: name.into(),
            muted: false,
            locked: false,
            clips: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Clip {
    pub id: Uuid,
    pub name: String,
    pub source: ClipSource,
    pub track_kind: TrackKind,
    pub start_frame: Frame,
    pub duration_frames: Frame,
    pub trim_start_frame: Frame,
    pub opacity: f32,
    pub volume: f32,
    pub effects: Vec<EffectInstance>,
}

impl Clip {
    pub fn media(
        name: impl Into<String>,
        media_id: Uuid,
        track_kind: TrackKind,
        start_frame: Frame,
        duration_frames: Frame,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source: ClipSource::Media { media_id },
            track_kind,
            start_frame,
            duration_frames,
            trim_start_frame: 0,
            opacity: 1.0,
            volume: 1.0,
            effects: Vec::new(),
        }
    }

    pub fn text(
        name: impl Into<String>,
        text: impl Into<String>,
        start_frame: Frame,
        duration_frames: Frame,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source: ClipSource::Text { text: text.into() },
            track_kind: TrackKind::Video,
            start_frame,
            duration_frames,
            trim_start_frame: 0,
            opacity: 1.0,
            volume: 0.0,
            effects: Vec::new(),
        }
    }

    pub fn end_frame(&self) -> Frame {
        self.start_frame + self.duration_frames
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClipSource {
    Media { media_id: Uuid },
    Text { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_timeline_range_splits_and_ripples() {
        let media_id = Uuid::new_v4();
        let mut timeline = Timeline::default();
        let clip = Clip::media("shot", media_id, TrackKind::Video, 0, 300);
        timeline.add_clip(0, clip).unwrap();

        timeline.remove_timeline_range(100, 200, true).unwrap();

        let clips = &timeline.tracks[0].clips;
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].start_frame, 0);
        assert_eq!(clips[0].duration_frames, 100);
        assert_eq!(clips[1].start_frame, 100);
        assert_eq!(clips[1].duration_frames, 100);
        assert_eq!(clips[1].trim_start_frame, 200);
    }
}
