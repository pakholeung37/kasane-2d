//! motion3 conversion between runtime IDs and persistent document UUIDs.
use std::collections::{BTreeMap, HashMap};

use kasane_core::document::{
    MotionClip, MotionEvent, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
use kasane_core::Document;
use kasane_live2d::motion3::{
    decode_motion3, encode_motion3, Motion3, Motion3Curve, Motion3Event, Motion3Meta,
    MotionPoint as WirePoint, MotionSegment as WireSegment, MotionTarget,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct MotionImport {
    pub candidate: Document,
    pub diagnostics: Vec<MotionDiagnostic>,
}

#[derive(Debug, Error)]
#[error("Motion {code} at {path}: {message}")]
pub struct MotionProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> MotionProjectError {
    MotionProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn child_id(parent: &str, kind: &str, index: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(parent.as_bytes());
    hash.update(kind.as_bytes());
    hash.update(index.to_le_bytes());
    let mut bytes: [u8; 16] = hash.finalize()[..16]
        .try_into()
        .expect("SHA-256 has 16 bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn point(point: WirePoint) -> MotionPoint {
    MotionPoint {
        time: point.time,
        value: point.value,
    }
}
fn wire_point(point: MotionPoint) -> WirePoint {
    WirePoint {
        time: point.time,
        value: point.value,
    }
}

fn segment(segment: WireSegment) -> MotionSegment {
    match segment {
        WireSegment::Linear { end } => MotionSegment::Linear { end: point(end) },
        WireSegment::Bezier {
            control1,
            control2,
            end,
        } => MotionSegment::Bezier {
            control1: point(control1),
            control2: point(control2),
            end: point(end),
        },
        WireSegment::Stepped { end } => MotionSegment::Stepped { end: point(end) },
        WireSegment::InverseStepped { end } => MotionSegment::InverseStepped { end: point(end) },
    }
}

fn wire_segment(segment: &MotionSegment) -> WireSegment {
    match segment {
        MotionSegment::Linear { end } => WireSegment::Linear {
            end: wire_point(*end),
        },
        MotionSegment::Bezier {
            control1,
            control2,
            end,
        } => WireSegment::Bezier {
            control1: wire_point(*control1),
            control2: wire_point(*control2),
            end: wire_point(*end),
        },
        MotionSegment::Stepped { end } => WireSegment::Stepped {
            end: wire_point(*end),
        },
        MotionSegment::InverseStepped { end } => WireSegment::InverseStepped {
            end: wire_point(*end),
        },
    }
}

fn namespace(document: &Document, clip: Option<&MotionClip>) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for id in document.parameter_order() {
        ids.insert(
            format!("parameter:{id}"),
            document
                .get_parameter(id)
                .expect("ordered parameter")
                .runtime_id
                .clone(),
        );
    }
    for id in document.part_order() {
        ids.insert(
            format!("part:{id}"),
            document
                .get_part(id)
                .expect("ordered part")
                .runtime_id
                .clone(),
        );
    }
    if let Some(clip) = clip {
        for track in &clip.tracks {
            if let MotionTrackTarget::Unresolved {
                category,
                runtime_id,
            } = &track.target
            {
                ids.insert(
                    format!("unresolved:{category}:{runtime_id}"),
                    runtime_id.clone(),
                );
            }
        }
    }
    ids
}

fn content_hash(clip: &MotionClip) -> String {
    let bytes = serde_json::to_vec(&(
        &clip.name,
        clip.duration,
        clip.fps,
        clip.looping,
        clip.restricted_beziers,
        clip.fade_in,
        clip.fade_out,
        &clip.tracks,
        &clip.events,
        &clip.extensions,
        &clip.meta_extensions,
    ))
    .expect("validated motion content serializes");
    format!("{:x}", Sha256::digest(bytes))
}

pub fn import_motion3(
    document: &Document,
    id: &str,
    name: &str,
    text: &str,
) -> Result<MotionImport, MotionProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let wire = decode_motion3(text).map_err(|e| error(e.code, e.path, e.message))?;
    let mut diagnostics = Vec::new();
    if !wire.counts_match() {
        diagnostics.push(MotionDiagnostic {
            code: "MOTION_COUNT_MISMATCH".into(),
            path: "$.Meta".into(),
            message: "stored counts differ from curve and event data; export will recompute them"
                .into(),
        });
    }
    let parameters: HashMap<_, _> = document
        .parameter_order()
        .iter()
        .map(|id| {
            (
                document
                    .get_parameter(id)
                    .expect("ordered parameter")
                    .runtime_id
                    .as_str(),
                id.as_str(),
            )
        })
        .collect();
    let parts: HashMap<_, _> = document
        .part_order()
        .iter()
        .map(|id| {
            (
                document
                    .get_part(id)
                    .expect("ordered part")
                    .runtime_id
                    .as_str(),
                id.as_str(),
            )
        })
        .collect();
    let tracks = wire
        .curves
        .into_iter()
        .enumerate()
        .map(|(index, curve)| {
            let target = match curve.target {
                MotionTarget::Model => MotionTrackTarget::Model {
                    runtime_id: curve.id,
                },
                MotionTarget::Parameter => {
                    if let Some(id) = parameters.get(curve.id.as_str()) {
                        MotionTrackTarget::Parameter {
                            parameter_id: (*id).into(),
                        }
                    } else {
                        diagnostics.push(MotionDiagnostic {
                            code: "UNRESOLVED_PARAMETER".into(),
                            path: format!("$.Curves[{index}].Id"),
                            message: format!("{} is absent from the model", curve.id),
                        });
                        MotionTrackTarget::Unresolved {
                            category: "Parameter".into(),
                            runtime_id: curve.id,
                        }
                    }
                }
                MotionTarget::PartOpacity => {
                    if let Some(id) = parts.get(curve.id.as_str()) {
                        MotionTrackTarget::PartOpacity {
                            part_id: (*id).into(),
                        }
                    } else {
                        diagnostics.push(MotionDiagnostic {
                            code: "UNRESOLVED_PART".into(),
                            path: format!("$.Curves[{index}].Id"),
                            message: format!("{} is absent from the model", curve.id),
                        });
                        MotionTrackTarget::Unresolved {
                            category: "PartOpacity".into(),
                            runtime_id: curve.id,
                        }
                    }
                }
            };
            MotionTrack {
                id: child_id(id, "track", index),
                target,
                initial: point(curve.initial),
                segments: std::sync::Arc::new(curve.segments.into_iter().map(segment).collect()),
                fade_in: curve.fade_in,
                fade_out: curve.fade_out,
                extensions: curve.extensions,
            }
        })
        .collect();
    let events = wire
        .user_data
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, event)| MotionEvent {
            id: child_id(id, "event", index),
            time: event.time,
            value: event.value,
            extensions: event.extensions,
        })
        .collect();
    let mut clip = MotionClip {
        id: id.into(),
        name: name.into(),
        duration: wire.meta.duration,
        fps: wire.meta.fps,
        looping: wire.meta.looping,
        restricted_beziers: wire.meta.restricted_beziers,
        fade_in: wire.meta.fade_in,
        fade_out: wire.meta.fade_out,
        tracks,
        events,
        extensions: wire.extensions,
        meta_extensions: wire.meta.extensions,
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    if clip.has_extensions() {
        clip.opaque_source_ids = Some(namespace(document, Some(&clip)));
        clip.opaque_source_content_hash = Some(content_hash(&clip));
    }
    let mut candidate = document.fork_candidate();
    let result = if candidate.get_motion(id).is_some() {
        candidate.replace_motion(clip)
    } else {
        candidate.create_motion(clip)
    };
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(MotionImport {
        candidate,
        diagnostics,
    })
}

pub fn export_motion3(document: &Document, id: &str) -> Result<String, MotionProjectError> {
    let clip = document
        .get_motion(id)
        .ok_or_else(|| error("MISSING_MOTION", "$", id))?;
    if clip.has_extensions() {
        if clip.opaque_source_ids.as_ref() != Some(&namespace(document, Some(clip))) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "runtime ID namespace changed since import",
            ));
        }
        if clip.opaque_source_content_hash.as_deref() != Some(content_hash(clip).as_str()) {
            return Err(error("OPAQUE_CONTENT_CHANGED", "$", "motion content changed since import; reimport to establish an opaque-field baseline"));
        }
    }
    let curves = clip
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let path = format!("$.Curves[{index}].Id");
            let (target, runtime_id) = match &track.target {
                MotionTrackTarget::Model { runtime_id } => {
                    (MotionTarget::Model, runtime_id.clone())
                }
                MotionTrackTarget::Parameter { parameter_id } => (
                    MotionTarget::Parameter,
                    document
                        .get_parameter(parameter_id)
                        .ok_or_else(|| error("MISSING_PARAMETER", &path, parameter_id))?
                        .runtime_id
                        .clone(),
                ),
                MotionTrackTarget::PartOpacity { part_id } => (
                    MotionTarget::PartOpacity,
                    document
                        .get_part(part_id)
                        .ok_or_else(|| error("MISSING_PART", &path, part_id))?
                        .runtime_id
                        .clone(),
                ),
                MotionTrackTarget::Unresolved {
                    category,
                    runtime_id,
                } => (
                    match category.as_str() {
                        "Parameter" => MotionTarget::Parameter,
                        "PartOpacity" => MotionTarget::PartOpacity,
                        _ => return Err(error("INVALID_TARGET", path, category)),
                    },
                    runtime_id.clone(),
                ),
            };
            Ok(Motion3Curve {
                target,
                id: runtime_id,
                initial: wire_point(track.initial),
                segments: track.segments.iter().map(wire_segment).collect(),
                fade_in: track.fade_in,
                fade_out: track.fade_out,
                extensions: track.extensions.clone(),
            })
        })
        .collect::<Result<Vec<_>, MotionProjectError>>()?;
    let events = clip
        .events
        .iter()
        .map(|event| Motion3Event {
            time: event.time,
            value: event.value.clone(),
            extensions: event.extensions.clone(),
        })
        .collect();
    let wire = Motion3 {
        version: 3,
        meta: Motion3Meta {
            duration: clip.duration,
            fps: clip.fps,
            looping: clip.looping,
            restricted_beziers: clip.restricted_beziers,
            curve_count: 0,
            total_segment_count: 0,
            total_point_count: 0,
            user_data_count: 0,
            total_user_data_size: 0,
            fade_in: clip.fade_in,
            fade_out: clip.fade_out,
            extensions: clip.meta_extensions.clone(),
        },
        curves,
        user_data: Some(events),
        extensions: clip.extensions.clone(),
    };
    encode_motion3(&wire).map_err(|e| error(e.code, e.path, e.message))
}
