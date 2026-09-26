//! Persistent motion clips. Playback time and queue state live outside Document.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotionPoint {
    pub time: f32,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MotionSegment {
    Linear {
        end: MotionPoint,
    },
    Bezier {
        control1: MotionPoint,
        control2: MotionPoint,
        end: MotionPoint,
    },
    Stepped {
        end: MotionPoint,
    },
    InverseStepped {
        end: MotionPoint,
    },
}

impl MotionSegment {
    pub fn end(&self) -> MotionPoint {
        match self {
            Self::Linear { end }
            | Self::Bezier { end, .. }
            | Self::Stepped { end }
            | Self::InverseStepped { end } => *end,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MotionTrackTarget {
    Model {
        runtime_id: String,
    },
    Parameter {
        parameter_id: String,
    },
    PartOpacity {
        part_id: String,
    },
    Unresolved {
        category: String,
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionTrack {
    pub id: String,
    pub target: MotionTrackTarget,
    pub initial: MotionPoint,
    pub segments: std::sync::Arc<Vec<MotionSegment>>,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionEvent {
    pub id: String,
    pub time: f32,
    pub value: String,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionClip {
    pub id: String,
    pub name: String,
    pub duration: f32,
    pub fps: f32,
    pub looping: bool,
    pub restricted_beziers: bool,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub tracks: Vec<MotionTrack>,
    pub events: Vec<MotionEvent>,
    pub extensions: BTreeMap<String, Value>,
    pub meta_extensions: BTreeMap<String, Value>,
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_source_content_hash: Option<String>,
}

/// model3 registration is separate from a clip: one file may appear in multiple groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionRegistration {
    pub clip_id: String,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub sound: Option<String>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionGroup {
    pub name: String,
    pub entries: Vec<MotionRegistration>,
}

impl MotionClip {
    /// Framework stage order, preserving authored order within each stage.
    /// Unresolved controls belong to the same stage as their resolved peers.
    pub fn tracks_in_evaluation_order(&self) -> impl Iterator<Item = &MotionTrack> {
        let stage = |target: &MotionTrackTarget| match target {
            MotionTrackTarget::Model { .. } => 0,
            MotionTrackTarget::Parameter { .. } => 1,
            MotionTrackTarget::PartOpacity { .. } => 2,
            MotionTrackTarget::Unresolved { category, .. } if category == "Parameter" => 1,
            MotionTrackTarget::Unresolved { .. } => 2,
        };
        (0..3).flat_map(move |order| {
            self.tracks
                .iter()
                .filter(move |track| stage(&track.target) == order)
        })
    }

    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || !self.meta_extensions.is_empty()
            || self.tracks.iter().any(|track| !track.extensions.is_empty())
            || self.events.iter().any(|event| !event.extensions.is_empty())
    }
}

impl Document {
    pub fn motion_groups(&self) -> &[MotionGroup] {
        &self.motion_groups
    }

    pub fn set_motion_groups(&mut self, groups: Vec<MotionGroup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let status = self.validate_motion_groups(&groups);
        if !status.is_ok() {
            return self.failed(status);
        }
        if self.motion_groups == groups {
            return self.failed(Status::ok());
        }
        self.motion_groups = groups;
        self.changed(ChangeKind::Metadata, Vec::new(), Vec::new())
    }

    fn validate_motion_groups(&self, groups: &[MotionGroup]) -> Status {
        let mut names = HashSet::new();
        for (index, group) in groups.iter().enumerate() {
            // Live2D model3 permits an empty Motion group key; Mao uses it for
            // its non-idle clips. It still has to be unique like any other key.
            if group.name.contains('\0') || !names.insert(&group.name) {
                return Status::error("INVALID_MOTION_GROUP", format!("groups[{index}]"));
            }
            for (entry_index, entry) in group.entries.iter().enumerate() {
                if !self.motions.contains_key(&entry.clip_id) {
                    return Status::error(
                        "MISSING_MOTION",
                        format!("groups[{index}].entries[{entry_index}]"),
                    );
                }
                if [entry.fade_in, entry.fade_out]
                    .into_iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v < 0.0)
                {
                    return Status::error(
                        "INVALID_MOTION_FADE",
                        format!("groups[{index}].entries[{entry_index}]"),
                    );
                }
                if entry
                    .sound
                    .as_ref()
                    .is_some_and(|sound| sound.is_empty() || sound.contains('\0'))
                {
                    return Status::error(
                        "INVALID_MOTION_SOUND",
                        format!("groups[{index}].entries[{entry_index}]"),
                    );
                }
            }
        }
        Status::ok()
    }
    pub fn motion_order(&self) -> &[String] {
        &self.motion_order
    }

    pub fn get_motion(&self, id: &str) -> Option<&MotionClip> {
        self.motions.get(id).map(std::sync::Arc::as_ref)
    }

    pub fn create_motion(&mut self, clip: MotionClip) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.contains_id(&clip.id) {
            return self.failed(Status::error("DUPLICATE_ID", &clip.id));
        }
        let status = self.validate_motion(&clip);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = clip.id.clone();
        self.motions.insert(id.clone(), std::sync::Arc::new(clip));
        self.motion_order.push(id.clone());
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    pub fn replace_motion(&mut self, clip: MotionClip) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.motions.contains_key(&clip.id) {
            return self.failed(Status::error("MISSING_MOTION", &clip.id));
        }
        let status = self.validate_motion(&clip);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = clip.id.clone();
        if self.get_motion(&id) == Some(&clip) {
            return self.failed(Status::ok());
        }
        self.motions.insert(id.clone(), std::sync::Arc::new(clip));
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    fn validate_motion(&self, clip: &MotionClip) -> Status {
        if !super::valid_uuid(&clip.id) {
            return Status::error("INVALID_MOTION_ID", &clip.id);
        }
        let nested_elsewhere = |id: &str| self.contains_motion_child_id(id, Some(&clip.id));
        if nested_elsewhere(&clip.id) {
            return Status::error("DUPLICATE_ID", &clip.id);
        }
        if clip.name.is_empty() || clip.name.contains('\0') {
            return Status::error("INVALID_MOTION_NAME", &clip.id);
        }
        if self
            .motions
            .values()
            .any(|other| other.id != clip.id && other.name == clip.name)
        {
            return Status::error("DUPLICATE_MOTION_NAME", &clip.name);
        }
        if !clip.duration.is_finite() || clip.duration <= 0.0 {
            return Status::error("INVALID_MOTION_DURATION", &clip.id);
        }
        if !clip.fps.is_finite() || clip.fps <= 0.0 {
            return Status::error("INVALID_MOTION_FPS", &clip.id);
        }
        if [clip.fade_in, clip.fade_out]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || value < 0.0)
        {
            return Status::error("INVALID_MOTION_FADE", &clip.id);
        }
        let mut ids = HashSet::new();
        for (index, track) in clip.tracks.iter().enumerate() {
            if !super::valid_uuid(&track.id)
                || !ids.insert(track.id.as_str())
                || track.id == clip.id
            {
                return Status::error(
                    "INVALID_MOTION_TRACK_ID",
                    format!("{}.tracks[{index}]", clip.id),
                );
            }
            if self.contains_top_level_id(&track.id) || nested_elsewhere(&track.id) {
                return Status::error("DUPLICATE_ID", &track.id);
            }
            if [track.fade_in, track.fade_out]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite() || value < 0.0)
            {
                return Status::error(
                    "INVALID_MOTION_FADE",
                    format!("{}.tracks[{index}]", clip.id),
                );
            }
            match &track.target {
                MotionTrackTarget::Parameter { parameter_id }
                    if self.get_parameter(parameter_id).is_none() =>
                {
                    return Status::error(
                        "MISSING_PARAMETER",
                        format!("{}.tracks[{index}]", clip.id),
                    );
                }
                MotionTrackTarget::PartOpacity { part_id } if self.get_part(part_id).is_none() => {
                    return Status::error("MISSING_PART", format!("{}.tracks[{index}]", clip.id));
                }
                MotionTrackTarget::Model { runtime_id }
                    if runtime_id.is_empty() || runtime_id.contains('\0') =>
                {
                    return Status::error(
                        "INVALID_MOTION_TARGET",
                        format!("{}.tracks[{index}]", clip.id),
                    );
                }
                MotionTrackTarget::Unresolved {
                    category,
                    runtime_id,
                } if !matches!(category.as_str(), "Parameter" | "PartOpacity")
                    || runtime_id.is_empty()
                    || runtime_id.contains('\0') =>
                {
                    return Status::error(
                        "INVALID_MOTION_TARGET",
                        format!("{}.tracks[{index}]", clip.id),
                    );
                }
                _ => {}
            }
            if !track.initial.time.is_finite()
                || !track.initial.value.is_finite()
                || track.initial.time < 0.0
                || track.initial.time > clip.duration
            {
                return Status::error(
                    "INVALID_MOTION_POINT",
                    format!("{}.tracks[{index}].initial", clip.id),
                );
            }
            // Cubism Editor may round the clip duration more coarsely than
            // its last curve timestamp (for example 9.23 vs 9.233 at 30 FPS).
            let end_tolerance = (0.5 / clip.fps).max(0.0001);
            let mut previous = track.initial.time;
            for (segment_index, segment) in track.segments.iter().enumerate() {
                let end = segment.end();
                if !end.time.is_finite()
                    || !end.value.is_finite()
                    || end.time <= previous
                    || end.time > clip.duration + end_tolerance
                {
                    return Status::error(
                        "INVALID_MOTION_POINT",
                        format!("{}.tracks[{index}].segments[{segment_index}]", clip.id),
                    );
                }
                if let MotionSegment::Bezier {
                    control1, control2, ..
                } = segment
                {
                    if ![control1.time, control1.value, control2.time, control2.value]
                        .into_iter()
                        .all(f32::is_finite)
                        || (clip.restricted_beziers
                            && !(previous <= control1.time
                                && control1.time <= control2.time
                                && control2.time <= end.time))
                    {
                        return Status::error(
                            "INVALID_MOTION_BEZIER",
                            format!("{}.tracks[{index}].segments[{segment_index}]", clip.id),
                        );
                    }
                }
                previous = end.time;
            }
        }
        for (index, event) in clip.events.iter().enumerate() {
            if !super::valid_uuid(&event.id)
                || !ids.insert(event.id.as_str())
                || event.id == clip.id
            {
                return Status::error(
                    "INVALID_MOTION_EVENT_ID",
                    format!("{}.events[{index}]", clip.id),
                );
            }
            if self.contains_top_level_id(&event.id) || nested_elsewhere(&event.id) {
                return Status::error("DUPLICATE_ID", &event.id);
            }
            if !event.time.is_finite() || event.time < 0.0 || event.time > clip.duration {
                return Status::error(
                    "INVALID_MOTION_EVENT_TIME",
                    format!("{}.events[{index}]", clip.id),
                );
            }
        }
        Status::ok()
    }

    pub(super) fn validate_motions(&self) -> Vec<StructureIssue> {
        let mut issues = Vec::new();
        for id in &self.motion_order {
            if let Some(clip) = self.motions.get(id) {
                let status = self.validate_motion(clip);
                if !status.is_ok() {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status,
                    });
                }
            }
        }
        let status = self.validate_motion_groups(&self.motion_groups);
        if !status.is_ok() {
            issues.push(StructureIssue {
                object_id: self.id.clone(),
                status,
            });
        }
        issues
    }
}
