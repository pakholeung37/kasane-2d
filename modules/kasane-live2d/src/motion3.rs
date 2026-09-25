//! Typed motion3 curves and IO-free Cubism JSON conversion.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion3 {
    #[serde(rename = "Version")]
    pub version: u32,
    #[serde(rename = "Meta")]
    pub meta: Motion3Meta,
    #[serde(rename = "Curves")]
    pub curves: Vec<Motion3Curve>,
    #[serde(rename = "UserData", default, skip_serializing_if = "Option::is_none")]
    pub user_data: Option<Vec<Motion3Event>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion3Meta {
    #[serde(rename = "Duration")]
    pub duration: f32,
    #[serde(rename = "Fps")]
    pub fps: f32,
    #[serde(rename = "Loop")]
    pub looping: bool,
    #[serde(rename = "AreBeziersRestricted")]
    pub restricted_beziers: bool,
    #[serde(rename = "CurveCount")]
    pub curve_count: usize,
    #[serde(rename = "TotalSegmentCount")]
    pub total_segment_count: usize,
    #[serde(rename = "TotalPointCount")]
    pub total_point_count: usize,
    #[serde(rename = "UserDataCount")]
    pub user_data_count: usize,
    #[serde(rename = "TotalUserDataSize")]
    pub total_user_data_size: usize,
    #[serde(
        rename = "FadeInTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fade_in: Option<f32>,
    #[serde(
        rename = "FadeOutTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fade_out: Option<f32>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionTarget {
    Model,
    Parameter,
    PartOpacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotionPoint {
    pub time: f32,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion3Curve {
    pub target: MotionTarget,
    pub id: String,
    pub initial: MotionPoint,
    pub segments: Vec<MotionSegment>,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion3Event {
    #[serde(rename = "Time")]
    pub time: f32,
    #[serde(rename = "Value")]
    pub value: String,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Motion3Wire {
    #[serde(rename = "Version")]
    version: u32,
    #[serde(rename = "Meta")]
    meta: Motion3Meta,
    #[serde(rename = "Curves")]
    curves: Vec<Motion3CurveWire>,
    #[serde(rename = "UserData", default, skip_serializing_if = "Option::is_none")]
    user_data: Option<Vec<Motion3Event>>,
    #[serde(flatten)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Motion3CurveWire {
    #[serde(rename = "Target")]
    target: MotionTarget,
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Segments")]
    segments: Vec<f32>,
    #[serde(
        rename = "FadeInTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    fade_in: Option<f32>,
    #[serde(
        rename = "FadeOutTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    fade_out: Option<f32>,
    #[serde(flatten)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("motion3 {code} at {path}: {message}")]
pub struct Motion3Error {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Motion3Error {
    Motion3Error {
        code,
        path: path.into(),
        message: message.into(),
    }
}

fn point(values: &[f32], index: usize, path: &str) -> Result<MotionPoint, Motion3Error> {
    let Some((&time, &value)) = values.get(index).zip(values.get(index + 1)) else {
        return Err(error(
            "TRUNCATED_SEGMENT",
            format!("{path}[{index}]"),
            "segment point is incomplete",
        ));
    };
    if !time.is_finite() || !value.is_finite() {
        return Err(error(
            "NONFINITE_SEGMENT",
            format!("{path}[{index}]"),
            "segment point must be finite",
        ));
    }
    Ok(MotionPoint { time, value })
}

fn decode_curve(raw: Motion3CurveWire, index: usize) -> Result<Motion3Curve, Motion3Error> {
    let path = format!("$.Curves[{index}].Segments");
    let initial = point(&raw.segments, 0, &path)?;
    if initial.time < 0.0 {
        return Err(error(
            "INVALID_SEGMENT_TIME",
            format!("{path}[0]"),
            "initial time must be nonnegative",
        ));
    }
    let mut previous = initial.time;
    let mut position = 2;
    let mut segments = Vec::new();
    while position < raw.segments.len() {
        let kind = raw.segments[position];
        let segment = match kind {
            0.0 => MotionSegment::Linear {
                end: point(&raw.segments, position + 1, &path)?,
            },
            1.0 => MotionSegment::Bezier {
                control1: point(&raw.segments, position + 1, &path)?,
                control2: point(&raw.segments, position + 3, &path)?,
                end: point(&raw.segments, position + 5, &path)?,
            },
            2.0 => MotionSegment::Stepped {
                end: point(&raw.segments, position + 1, &path)?,
            },
            3.0 => MotionSegment::InverseStepped {
                end: point(&raw.segments, position + 1, &path)?,
            },
            _ => {
                return Err(error(
                    "INVALID_SEGMENT_TYPE",
                    format!("{path}[{position}]"),
                    "segment type must be 0, 1, 2, or 3",
                ))
            }
        };
        let end = segment.end();
        if end.time <= previous {
            return Err(error(
                "INVALID_SEGMENT_TIME",
                format!("{path}[{}]", position + if kind == 1.0 { 5 } else { 1 }),
                "segment endpoints must increase from a nonnegative initial time",
            ));
        }
        previous = end.time;
        position += if kind == 1.0 { 7 } else { 3 };
        segments.push(segment);
    }
    Ok(Motion3Curve {
        target: raw.target,
        id: raw.id,
        initial,
        segments,
        fade_in: raw.fade_in,
        fade_out: raw.fade_out,
        extensions: raw.extensions,
    })
}

fn encode_curve(curve: &Motion3Curve) -> Motion3CurveWire {
    let mut values = vec![curve.initial.time, curve.initial.value];
    for segment in &curve.segments {
        match segment {
            MotionSegment::Linear { end } => values.extend([0.0, end.time, end.value]),
            MotionSegment::Bezier {
                control1,
                control2,
                end,
            } => values.extend([
                1.0,
                control1.time,
                control1.value,
                control2.time,
                control2.value,
                end.time,
                end.value,
            ]),
            MotionSegment::Stepped { end } => values.extend([2.0, end.time, end.value]),
            MotionSegment::InverseStepped { end } => values.extend([3.0, end.time, end.value]),
        }
    }
    Motion3CurveWire {
        target: curve.target,
        id: curve.id.clone(),
        segments: values,
        fade_in: curve.fade_in,
        fade_out: curve.fade_out,
        extensions: curve.extensions.clone(),
    }
}

impl Motion3 {
    pub fn actual_counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.curves.len(),
            self.curves.iter().map(|curve| curve.segments.len()).sum(),
            self.curves
                .iter()
                .map(|curve| {
                    1 + curve
                        .segments
                        .iter()
                        .map(|segment| {
                            if matches!(segment, MotionSegment::Bezier { .. }) {
                                3
                            } else {
                                1
                            }
                        })
                        .sum::<usize>()
                })
                .sum(),
            self.user_data.as_ref().map_or(0, Vec::len),
            self.user_data.as_ref().map_or(0, |events| {
                events.iter().map(|event| event.value.len()).sum()
            }),
        )
    }

    pub fn counts_match(&self) -> bool {
        self.actual_counts()
            == (
                self.meta.curve_count,
                self.meta.total_segment_count,
                self.meta.total_point_count,
                self.meta.user_data_count,
                self.meta.total_user_data_size,
            )
    }
}

fn validate(motion: &Motion3, writer: bool) -> Result<(), Motion3Error> {
    if motion.version != 3 {
        return Err(error(
            "INVALID_VERSION",
            "$.Version",
            "motion3 Version must be 3",
        ));
    }
    if !motion.meta.duration.is_finite() || motion.meta.duration <= 0.0 {
        return Err(error(
            "INVALID_DURATION",
            "$.Meta.Duration",
            "duration must be positive and finite",
        ));
    }
    if !motion.meta.fps.is_finite() || motion.meta.fps <= 0.0 {
        return Err(error(
            "INVALID_FPS",
            "$.Meta.Fps",
            "fps must be positive and finite",
        ));
    }
    for (label, fade) in [
        ("FadeInTime", motion.meta.fade_in),
        ("FadeOutTime", motion.meta.fade_out),
    ] {
        if fade.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(error(
                "INVALID_FADE",
                format!("$.Meta.{label}"),
                "fade must be finite and nonnegative",
            ));
        }
    }
    for (index, curve) in motion.curves.iter().enumerate() {
        if curve.id.is_empty() || curve.id.contains('\0') {
            return Err(error(
                "INVALID_CURVE_ID",
                format!("$.Curves[{index}].Id"),
                "curve ID is empty or contains NUL",
            ));
        }
        for (label, fade) in [
            ("FadeInTime", curve.fade_in),
            ("FadeOutTime", curve.fade_out),
        ] {
            if fade.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(error(
                    "INVALID_FADE",
                    format!("$.Curves[{index}].{label}"),
                    "fade must be finite and nonnegative",
                ));
            }
        }
        let raw = encode_curve(curve);
        decode_curve(raw, index)?;
        if curve.initial.time > motion.meta.duration {
            return Err(error(
                "CURVE_EXCEEDS_DURATION",
                format!("$.Curves[{index}].Segments[0]"),
                "initial time exceeds duration",
            ));
        }
        // Cubism Editor rounds Meta.Duration separately from curve timestamps.
        // Its own Mao sample has Duration 9.23 and final points at 9.233.
        // Accept at most half a frame of rounding, while still rejecting
        // endpoints that materially extend past the clip.
        let end_tolerance = (0.5 / motion.meta.fps).max(0.0001);
        if curve
            .segments
            .last()
            .is_some_and(|segment| segment.end().time > motion.meta.duration + end_tolerance)
        {
            return Err(error(
                "CURVE_EXCEEDS_DURATION",
                format!("$.Curves[{index}].Segments"),
                "curve endpoint exceeds duration",
            ));
        }
        if motion.meta.restricted_beziers {
            let mut previous = curve.initial.time;
            for segment in &curve.segments {
                let end = segment.end().time;
                if let MotionSegment::Bezier {
                    control1, control2, ..
                } = segment
                {
                    if !(previous <= control1.time
                        && control1.time <= control2.time
                        && control2.time <= end)
                    {
                        return Err(error(
                            "INVALID_RESTRICTED_BEZIER",
                            format!("$.Curves[{index}].Segments"),
                            "restricted Bezier control times must be ordered within the segment",
                        ));
                    }
                }
                previous = end;
            }
        }
    }
    if let Some(events) = &motion.user_data {
        for (index, event) in events.iter().enumerate() {
            if !event.time.is_finite() || event.time < 0.0 || event.time > motion.meta.duration {
                return Err(error(
                    "INVALID_EVENT_TIME",
                    format!("$.UserData[{index}].Time"),
                    "event time is outside duration",
                ));
            }
        }
    }
    if writer {
        for (path, extensions) in std::iter::once(("$".to_string(), &motion.extensions))
            .chain(std::iter::once((
                "$.Meta".to_string(),
                &motion.meta.extensions,
            )))
            .chain(
                motion
                    .curves
                    .iter()
                    .enumerate()
                    .map(|(i, curve)| (format!("$.Curves[{i}]"), &curve.extensions)),
            )
            .chain(
                motion
                    .user_data
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(|(i, event)| (format!("$.UserData[{i}]"), &event.extensions)),
            )
        {
            for (key, value) in extensions {
                let known: &[&str] = if path == "$" {
                    &["Version", "Meta", "Curves", "UserData"]
                } else if path == "$.Meta" {
                    &[
                        "Duration",
                        "Fps",
                        "Loop",
                        "AreBeziersRestricted",
                        "CurveCount",
                        "TotalSegmentCount",
                        "TotalPointCount",
                        "UserDataCount",
                        "TotalUserDataSize",
                        "FadeInTime",
                        "FadeOutTime",
                    ]
                } else if path.starts_with("$.Curves[") {
                    &["Target", "Id", "Segments", "FadeInTime", "FadeOutTime"]
                } else {
                    &["Time", "Value"]
                };
                if known.contains(&key.as_str()) {
                    return Err(error(
                        "EXTENSION_KEY_COLLISION",
                        format!("{path}.{key}"),
                        "extension shadows a typed field",
                    ));
                }
                if contains_number(value) {
                    return Err(error(
                        "UNSUPPORTED_EXTENSION_NUMBER",
                        format!("{path}.{key}"),
                        "numeric unknown fields lack a verified Framework encoding",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn contains_number(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::Array(items) => items.iter().any(contains_number),
        Value::Object(items) => items.values().any(contains_number),
        _ => false,
    }
}

pub fn decode_motion3(text: &str) -> Result<Motion3, Motion3Error> {
    crate::cdi3::check_json_members(text).map_err(|failure| {
        error(
            "INVALID_JSON",
            failure.path().unwrap_or("$"),
            failure.to_string(),
        )
    })?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let wire: Motion3Wire =
        serde_path_to_error::deserialize(&mut deserializer).map_err(|failure| {
            error(
                "INVALID_FIELD",
                format!("$.{}", failure.path()),
                failure.inner().to_string(),
            )
        })?;
    deserializer
        .end()
        .map_err(|failure| error("INVALID_JSON", "$", failure.to_string()))?;
    let curves = wire
        .curves
        .into_iter()
        .enumerate()
        .map(|(index, curve)| decode_curve(curve, index))
        .collect::<Result<Vec<_>, _>>()?;
    let motion = Motion3 {
        version: wire.version,
        meta: wire.meta,
        curves,
        user_data: wire.user_data,
        extensions: wire.extensions,
    };
    validate(&motion, false)?;
    Ok(motion)
}

/// Canonicalize counts and emit line-broken arrays for the local Framework
/// JSON parser, which rejects some valid compact numeric terminators.
pub fn encode_motion3(motion: &Motion3) -> Result<String, Motion3Error> {
    validate(motion, true)?;
    let mut meta = motion.meta.clone();
    let (curves, segments, points, events, bytes) = motion.actual_counts();
    meta.curve_count = curves;
    meta.total_segment_count = segments;
    meta.total_point_count = points;
    meta.user_data_count = events;
    meta.total_user_data_size = bytes;
    let wire = Motion3Wire {
        version: motion.version,
        meta,
        curves: motion.curves.iter().map(encode_curve).collect(),
        user_data: motion.user_data.clone(),
        extensions: motion.extensions.clone(),
    };
    let value = serde_json::to_value(wire)
        .map_err(|failure| error("SERIALIZATION", "$", failure.to_string()))?;
    let mut output = String::new();
    write_value(&value, 0, &mut output);
    output.push('\n');
    Ok(output)
}

pub(crate) fn write_value(value: &Value, depth: usize, output: &mut String) {
    match value {
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push('\n');
                output.push_str(&"  ".repeat(depth + 1));
                write_value(item, depth + 1, output);
            }
            output.push('\n');
            output.push_str(&"  ".repeat(depth));
            output.push(']');
        }
        Value::Object(items) => {
            output.push('{');
            for (index, (key, item)) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push('\n');
                output.push_str(&"  ".repeat(depth + 1));
                output.push_str(&serde_json::to_string(key).expect("JSON key serializes"));
                output.push_str(": ");
                write_value(item, depth + 1, output);
            }
            output.push('\n');
            output.push_str(&"  ".repeat(depth));
            output.push('}');
        }
        Value::Number(number) => {
            let text = number.to_string();
            if text.contains(['e', 'E']) {
                output.push_str(&format!("{:.17}", number.as_f64().expect("numeric JSON")));
            } else {
                output.push_str(&text);
            }
        }
        _ => output.push_str(&serde_json::to_string(value).expect("JSON value serializes")),
    }
}
