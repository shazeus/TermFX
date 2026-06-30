use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, TermFxError};

use super::effect::{EffectInstance, TransformKeyframe};
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
    #[serde(default)]
    pub markers: Vec<TimelineMarker>,
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
            markers: Vec::new(),
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

    pub fn clip_track_index(&self, id: Uuid) -> Result<usize> {
        self.tracks
            .iter()
            .find(|track| track.clips.iter().any(|clip| clip.id == id))
            .map(|track| track.index)
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

    pub fn add_track(&mut self, kind: TrackKind, name: Option<String>) -> usize {
        let index = self
            .tracks
            .iter()
            .map(|track| track.index)
            .max()
            .map(|index| index + 1)
            .unwrap_or(0);
        let default_name = match kind {
            TrackKind::Video => format!(
                "V{}",
                self.tracks.iter().filter(|t| t.kind == kind).count() + 1
            ),
            TrackKind::Audio => format!(
                "A{}",
                self.tracks.iter().filter(|t| t.kind == kind).count() + 1
            ),
        };
        self.tracks
            .push(Track::new(index, kind, name.unwrap_or(default_name)));
        index
    }

    pub fn set_track_state(
        &mut self,
        track_index: usize,
        muted: Option<bool>,
        locked: Option<bool>,
        name: Option<String>,
    ) -> Result<()> {
        let track = self.track_mut(track_index)?;
        if let Some(muted) = muted {
            track.muted = muted;
        }
        if let Some(locked) = locked {
            track.locked = locked;
        }
        if let Some(name) = name {
            track.name = name;
        }
        Ok(())
    }

    pub fn add_marker(
        &mut self,
        frame: Frame,
        label: String,
        color: Option<String>,
        note: Option<String>,
    ) -> Uuid {
        let marker = TimelineMarker {
            id: Uuid::new_v4(),
            frame,
            label,
            color: color.unwrap_or_else(|| "yellow".to_string()),
            note: note.unwrap_or_default(),
        };
        let id = marker.id;
        self.markers.push(marker);
        self.markers.sort_by_key(|marker| (marker.frame, marker.id));
        id
    }

    pub fn remove_marker(&mut self, marker_id: Uuid) -> Result<TimelineMarker> {
        let position = self
            .markers
            .iter()
            .position(|marker| marker.id == marker_id)
            .ok_or_else(|| {
                TermFxError::InvalidMcpRequest(format!("missing marker: {marker_id}"))
            })?;
        Ok(self.markers.remove(position))
    }

    pub fn remove_clip(&mut self, clip_id: Uuid) -> Result<Clip> {
        for track in &mut self.tracks {
            if let Some(position) = track.clips.iter().position(|clip| clip.id == clip_id) {
                return Ok(track.clips.remove(position));
            }
        }

        Err(TermFxError::MissingClip(clip_id))
    }

    pub fn move_clip_to_track(&mut self, clip_id: Uuid, target_track_index: usize) -> Result<()> {
        let target_kind = self
            .tracks
            .iter()
            .find(|track| track.index == target_track_index)
            .map(|track| track.kind)
            .ok_or(TermFxError::MissingTrack(target_track_index))?;
        let clip = self.remove_clip(clip_id)?;
        if clip.track_kind != target_kind {
            return Err(TermFxError::TrackKindMismatch {
                track_index: target_track_index,
            });
        }
        self.add_clip(target_track_index, clip)?;
        Ok(())
    }

    pub fn split_clip_at_timeline_frame(
        &mut self,
        clip_id: Uuid,
        split_frame: Frame,
    ) -> Result<Uuid> {
        let track_index = self.clip_track_index(clip_id)?;
        let track = self.track_mut(track_index)?;
        let position = track
            .clips
            .iter()
            .position(|clip| clip.id == clip_id)
            .ok_or(TermFxError::MissingClip(clip_id))?;
        let clip_start = track.clips[position].start_frame;
        let clip_end = track.clips[position].end_frame();
        if split_frame <= clip_start || split_frame >= clip_end {
            return Err(TermFxError::InvalidRange {
                start: split_frame,
                end: clip_end,
            });
        }

        let right_duration = clip_end - split_frame;
        let left_duration = split_frame - clip_start;
        let mut right = track.clips[position].clone();
        right.id = Uuid::new_v4();
        right.name = format!("{} (split)", right.name);
        right.start_frame = split_frame;
        right.trim_start_frame += left_duration;
        right.duration_frames = right_duration;
        track.clips[position].duration_frames = left_duration;

        let right_id = right.id;
        track.clips.push(right);
        track.clips.sort_by_key(|clip| (clip.start_frame, clip.id));
        Ok(right_id)
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TimelineMarker {
    pub id: Uuid,
    pub frame: Frame,
    pub label: String,
    pub color: String,
    pub note: String,
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
    #[serde(default = "default_speed")]
    pub speed: f32,
    pub effects: Vec<EffectInstance>,
    #[serde(default)]
    pub keyframes: Vec<TransformKeyframe>,
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
            speed: 1.0,
            effects: Vec::new(),
            keyframes: Vec::new(),
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
            speed: 1.0,
            effects: Vec::new(),
            keyframes: Vec::new(),
        }
    }

    pub fn end_frame(&self) -> Frame {
        self.start_frame + self.duration_frames
    }

    pub fn add_keyframe(&mut self, keyframe: TransformKeyframe) -> Uuid {
        let id = keyframe.id;
        self.keyframes.push(keyframe);
        self.keyframes
            .sort_by_key(|keyframe| (keyframe.frame, keyframe.id));
        id
    }

    pub fn remove_keyframe(&mut self, keyframe_id: Uuid) -> Result<TransformKeyframe> {
        let position = self
            .keyframes
            .iter()
            .position(|keyframe| keyframe.id == keyframe_id)
            .ok_or_else(|| {
                TermFxError::InvalidMcpRequest(format!("missing keyframe: {keyframe_id}"))
            })?;
        Ok(self.keyframes.remove(position))
    }
}

fn default_speed() -> f32 {
    1.0
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

    #[test]
    fn split_clip_keeps_source_offsets() {
        let media_id = Uuid::new_v4();
        let mut timeline = Timeline::default();
        let mut clip = Clip::media("shot", media_id, TrackKind::Video, 30, 120);
        clip.trim_start_frame = 15;
        let clip_id = clip.id;
        timeline.add_clip(0, clip).unwrap();

        let right_id = timeline.split_clip_at_timeline_frame(clip_id, 90).unwrap();
        let left = timeline.clip(clip_id).unwrap();
        let right = timeline.clip(right_id).unwrap();

        assert_eq!(left.start_frame, 30);
        assert_eq!(left.duration_frames, 60);
        assert_eq!(left.trim_start_frame, 15);
        assert_eq!(right.start_frame, 90);
        assert_eq!(right.duration_frames, 60);
        assert_eq!(right.trim_start_frame, 75);
    }

    #[test]
    fn tracks_markers_and_keyframes_are_mutable() {
        let mut timeline = Timeline::default();
        let video_track = timeline.add_track(TrackKind::Video, Some("Adjustment".to_string()));
        assert_eq!(video_track, 3);

        timeline
            .set_track_state(video_track, Some(true), Some(false), None)
            .unwrap();
        assert!(timeline.track_mut(video_track).unwrap().muted);

        let marker_id =
            timeline.add_marker(48, "Beat".to_string(), None, Some("cut here".to_string()));
        assert_eq!(timeline.markers.len(), 1);
        assert_eq!(timeline.remove_marker(marker_id).unwrap().label, "Beat");

        let mut clip = Clip::text("title", "Hello", 0, 60);
        let mut keyframe = TransformKeyframe::new(30);
        keyframe.x = 120.0;
        let keyframe_id = clip.add_keyframe(keyframe);
        assert_eq!(clip.keyframes.len(), 1);
        assert_eq!(clip.remove_keyframe(keyframe_id).unwrap().x, 120.0);
    }
}
