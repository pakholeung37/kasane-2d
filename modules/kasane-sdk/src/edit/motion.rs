//! Transactional motion editing. SDK errors abort the current edit. Timeline helpers
//! reject opaque extensions with `OPAQUE_EDIT_REQUIRES_IMPORT`; reimport to establish
//! a new baseline. Missing clip/track errors are `MISSING_MOTION`/`MISSING_MOTION_TRACK`.
use super::EditSession;
use crate::types::SdkError;
use kasane_core::document::{
    MotionClip, MotionEvent, MotionGroup, MotionPoint, MotionSegment, MotionTrack,
};
use kasane_core::ChangeKind;
use kasane_project::{import_motion3, MotionDiagnostic, MotionProjectError};

fn project_error(error: MotionProjectError, operation: &'static str) -> SdkError {
    let mut result = SdkError::new(&error.code, &error.message, operation);
    result.field_path = Some(error.path.into());
    result
}

impl EditSession<'_> {
    fn mutate_motion(
        &mut self,
        id: &str,
        operation: &'static str,
        change: impl FnOnce(&mut MotionClip) -> Result<(), SdkError>,
    ) -> Result<(), SdkError> {
        self.ensure_active(operation)?;
        let mut clip = self
            .candidate_document()
            .get_motion(id)
            .cloned()
            .ok_or_else(|| self.abort_with(SdkError::new("MISSING_MOTION", id, operation)))?;
        if clip.has_extensions() {
            return Err(self.abort_with(SdkError::new(
                "OPAQUE_EDIT_REQUIRES_IMPORT",
                "Reimport motion3 to edit a clip with unknown fields",
                operation,
            )));
        }
        change(&mut clip).map_err(|error| self.abort_with(error))?;
        self.replace_motion(clip)
    }

    /// Append a typed track to a clip UUID. Duplicate track IDs return `DUPLICATE_ID`.
    /// Validates the complete clip; missing clips return `MISSING_MOTION`.
    pub fn create_motion_track(&mut self, id: &str, track: MotionTrack) -> Result<(), SdkError> {
        self.mutate_motion(id, "create_motion_track", |clip| {
            if clip.tracks.iter().any(|item| item.id == track.id) {
                return Err(SdkError::new(
                    "DUPLICATE_ID",
                    &track.id,
                    "create_motion_track",
                ));
            }
            clip.tracks.push(track);
            Ok(())
        })
    }

    /// Replace a track identified by `track.id`, preserving its position in the clip.
    /// An absent track returns `MISSING_MOTION_TRACK`; validates the complete clip.
    pub fn replace_motion_track(&mut self, id: &str, track: MotionTrack) -> Result<(), SdkError> {
        self.mutate_motion(id, "replace_motion_track", |clip| {
            let existing = clip
                .tracks
                .iter_mut()
                .find(|item| item.id == track.id)
                .ok_or_else(|| {
                    SdkError::new("MISSING_MOTION_TRACK", &track.id, "replace_motion_track")
                })?;
            *existing = track;
            Ok(())
        })
    }

    /// Replace `segments[index]`; the index is zero-based and excludes the initial point.
    /// An absent index returns `MISSING_MOTION_SEGMENT`. Validates the complete clip.
    pub fn set_motion_segment(
        &mut self,
        id: &str,
        track_id: &str,
        index: usize,
        segment: MotionSegment,
    ) -> Result<(), SdkError> {
        self.mutate_motion(id, "set_motion_segment", |clip| {
            let track = clip
                .tracks
                .iter_mut()
                .find(|item| item.id == track_id)
                .ok_or_else(|| {
                    SdkError::new("MISSING_MOTION_TRACK", track_id, "set_motion_segment")
                })?;
            let slot = std::sync::Arc::make_mut(&mut track.segments)
                .get_mut(index)
                .ok_or_else(|| {
                    SdkError::new(
                        "MISSING_MOTION_SEGMENT",
                        &index.to_string(),
                        "set_motion_segment",
                    )
                })?;
            *slot = segment;
            Ok(())
        })
    }

    /// Insert before a zero-based segment index; `segments.len()` appends.
    /// Larger indices return `MISSING_MOTION_SEGMENT`. Does not reorder points;
    /// the resulting clip must satisfy point ordering and Bezier constraints.
    pub fn insert_motion_segment(
        &mut self,
        id: &str,
        track_id: &str,
        index: usize,
        segment: MotionSegment,
    ) -> Result<(), SdkError> {
        self.mutate_motion(id, "insert_motion_segment", |clip| {
            let track = clip
                .tracks
                .iter_mut()
                .find(|item| item.id == track_id)
                .ok_or_else(|| {
                    SdkError::new("MISSING_MOTION_TRACK", track_id, "insert_motion_segment")
                })?;
            if index > track.segments.len() {
                return Err(SdkError::new(
                    "MISSING_MOTION_SEGMENT",
                    &index.to_string(),
                    "insert_motion_segment",
                ));
            }
            std::sync::Arc::make_mut(&mut track.segments).insert(index, segment);
            Ok(())
        })
    }

    /// Remove a track UUID and its points; absent tracks return `MISSING_MOTION_TRACK`.
    pub fn remove_motion_track(&mut self, id: &str, track_id: &str) -> Result<(), SdkError> {
        self.mutate_motion(id, "remove_motion_track", |clip| {
            let count = clip.tracks.len();
            clip.tracks.retain(|track| track.id != track_id);
            if clip.tracks.len() == count {
                return Err(SdkError::new(
                    "MISSING_MOTION_TRACK",
                    track_id,
                    "remove_motion_track",
                ));
            }
            Ok(())
        })
    }

    /// Move key 0 (the initial point) or endpoint `index - 1` for keys 1..=segment count.
    /// Bezier control points remain unchanged. Out-of-range indices return
    /// `MISSING_MOTION_SEGMENT`; the complete clip is validated after the edit.
    pub fn move_motion_key(
        &mut self,
        id: &str,
        track_id: &str,
        index: usize,
        point: MotionPoint,
    ) -> Result<(), SdkError> {
        self.mutate_motion(id, "move_motion_key", |clip| {
            let track = clip
                .tracks
                .iter_mut()
                .find(|item| item.id == track_id)
                .ok_or_else(|| {
                    SdkError::new("MISSING_MOTION_TRACK", track_id, "move_motion_key")
                })?;
            if index == 0 {
                track.initial = point;
            } else {
                let segment = std::sync::Arc::make_mut(&mut track.segments)
                    .get_mut(index - 1)
                    .ok_or_else(|| {
                        SdkError::new(
                            "MISSING_MOTION_SEGMENT",
                            &index.to_string(),
                            "move_motion_key",
                        )
                    })?;
                match segment {
                    MotionSegment::Linear { end }
                    | MotionSegment::Stepped { end }
                    | MotionSegment::InverseStepped { end }
                    | MotionSegment::Bezier { end, .. } => *end = point,
                }
            }
            Ok(())
        })
    }

    /// Replace an event by UUID in place, or append a new event.
    /// Event time is finite seconds in `[0, duration]`; otherwise returns `INVALID_MOTION_EVENT_TIME`.
    pub fn set_motion_event(&mut self, id: &str, event: MotionEvent) -> Result<(), SdkError> {
        self.mutate_motion(id, "set_motion_event", |clip| {
            if let Some(slot) = clip.events.iter_mut().find(|item| item.id == event.id) {
                *slot = event;
            } else {
                clip.events.push(event);
            }
            Ok(())
        })
    }

    /// Remove an event UUID; absent events return `MISSING_MOTION_EVENT`, not success.
    pub fn remove_motion_event(&mut self, id: &str, event_id: &str) -> Result<(), SdkError> {
        self.mutate_motion(id, "remove_motion_event", |clip| {
            let count = clip.events.len();
            clip.events.retain(|event| event.id != event_id);
            if clip.events.len() == count {
                return Err(SdkError::new(
                    "MISSING_MOTION_EVENT",
                    event_id,
                    "remove_motion_event",
                ));
            }
            Ok(())
        })
    }

    /// Replace positive finite duration (seconds), FPS, loop flag and optional fades.
    /// Fades must be finite nonnegative seconds; `None` clears the override. Existing
    /// points/events are not rescaled, and the resulting clip must still validate.
    pub fn set_motion_timing(
        &mut self,
        id: &str,
        duration: f32,
        fps: f32,
        looping: bool,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    ) -> Result<(), SdkError> {
        self.mutate_motion(id, "set_motion_timing", |clip| {
            clip.duration = duration;
            clip.fps = fps;
            clip.looping = looping;
            clip.fade_in = fade_in;
            clip.fade_out = fade_out;
            Ok(())
        })
    }
    /// Create a validated persistent clip with a new UUID in the current transaction.
    pub fn create_motion(&mut self, clip: MotionClip) -> Result<(), SdkError> {
        self.ensure_active("create_motion")?;
        let id = clip.id.clone();
        let result = self.document().create_motion(clip);
        self.record(result, "create_motion", &id)
    }

    /// Replace an existing clip by UUID and validate its complete contents.
    /// Unlike timeline helpers, this low-level replacement accepts opaque records;
    /// strict export checks their preserved import baseline.
    pub fn replace_motion(&mut self, clip: MotionClip) -> Result<(), SdkError> {
        self.ensure_active("replace_motion")?;
        let id = clip.id.clone();
        let result = self.document().replace_motion(clip);
        self.record(result, "replace_motion", &id)
    }

    /// Replace ordered model3 registrations; entries reference existing clip UUIDs.
    /// One clip may appear in multiple entries with independent fades and Sound paths.
    pub fn set_motion_groups(&mut self, groups: Vec<MotionGroup>) -> Result<(), SdkError> {
        self.ensure_active("set_motion_groups")?;
        let result = self.document().set_motion_groups(groups);
        self.record(result, "set_motion_groups", "model3.Motions")
    }

    /// Import or replace a motion3 asset by UUID and name in the current transaction.
    /// Resolves runtime IDs to project UUIDs; unresolved targets are retained and returned
    /// as diagnostics. Malformed input aborts the edit with a structured SDK error.
    pub fn import_motion3(
        &mut self,
        id: &str,
        name: &str,
        text: &str,
    ) -> Result<Vec<MotionDiagnostic>, SdkError> {
        self.ensure_active("import_motion3")?;
        let imported = import_motion3(self.candidate_document(), id, name, text)
            .map_err(|error| self.abort_with(project_error(error, "import_motion3")))?;
        self.candidate = Some(imported.candidate);
        self.kind = super::merge_kind(self.kind, ChangeKind::Metadata);
        if !self.object_ids.iter().any(|item| item == id) {
            self.object_ids.push(id.into());
        }
        Ok(imported.diagnostics)
    }
}
