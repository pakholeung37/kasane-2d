//! Physics definitions and invariants shared by authoring and evaluation.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsVector {
    #[serde(rename = "X")]
    pub x: f32,
    #[serde(rename = "Y")]
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsForces {
    #[serde(rename = "Gravity")]
    pub gravity: PhysicsVector,
    #[serde(rename = "Wind")]
    pub wind: PhysicsVector,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsDictionaryEntry {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsMeta {
    #[serde(rename = "PhysicsSettingCount")]
    pub setting_count: usize,
    #[serde(rename = "TotalInputCount")]
    pub input_count: usize,
    #[serde(rename = "TotalOutputCount")]
    pub output_count: usize,
    #[serde(rename = "VertexCount")]
    pub vertex_count: usize,
    #[serde(rename = "EffectiveForces")]
    pub forces: PhysicsForces,
    #[serde(rename = "PhysicsDictionary")]
    pub dictionary: Vec<PhysicsDictionaryEntry>,
    #[serde(rename = "Fps", default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f32>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsParameterRef {
    #[serde(rename = "Target")]
    pub target: String,
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsAxis {
    X,
    Y,
    Angle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsInput {
    #[serde(rename = "Source")]
    pub source: PhysicsParameterRef,
    #[serde(rename = "Weight")]
    pub weight: f32,
    #[serde(rename = "Type")]
    pub kind: PhysicsAxis,
    #[serde(rename = "Reflect")]
    pub reflect: bool,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsOutput {
    #[serde(rename = "Destination")]
    pub destination: PhysicsParameterRef,
    #[serde(rename = "VertexIndex")]
    pub vertex_index: usize,
    #[serde(rename = "Scale")]
    pub scale: f32,
    #[serde(rename = "Weight")]
    pub weight: f32,
    #[serde(rename = "Type")]
    pub kind: PhysicsAxis,
    #[serde(rename = "Reflect")]
    pub reflect: bool,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsVertex {
    #[serde(rename = "Position")]
    pub position: PhysicsVector,
    #[serde(rename = "Mobility")]
    pub mobility: f32,
    #[serde(rename = "Delay")]
    pub delay: f32,
    #[serde(rename = "Acceleration")]
    pub acceleration: f32,
    #[serde(rename = "Radius")]
    pub radius: f32,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsRange {
    #[serde(rename = "Minimum")]
    pub minimum: f32,
    #[serde(rename = "Default")]
    pub default: f32,
    #[serde(rename = "Maximum")]
    pub maximum: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsNormalization {
    #[serde(rename = "Position")]
    pub position: PhysicsRange,
    #[serde(rename = "Angle")]
    pub angle: PhysicsRange,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsSetting {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Input")]
    pub inputs: Vec<PhysicsInput>,
    #[serde(rename = "Output")]
    pub outputs: Vec<PhysicsOutput>,
    #[serde(rename = "Vertices")]
    pub vertices: Vec<PhysicsVertex>,
    #[serde(rename = "Normalization")]
    pub normalization: PhysicsNormalization,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsDefinition {
    #[serde(rename = "Version")]
    pub version: u32,
    #[serde(rename = "Meta")]
    pub meta: PhysicsMeta,
    #[serde(rename = "PhysicsSettings")]
    pub settings: Vec<PhysicsSetting>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("physics3 {code} at {path}: {message}")]
pub struct PhysicsValidationError {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

fn error(
    code: &'static str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> PhysicsValidationError {
    PhysicsValidationError {
        code,
        path: path.into(),
        message: message.into(),
    }
}

fn finite(value: f32, path: String) -> Result<(), PhysicsValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(error(
            "NONFINITE_NUMBER",
            path,
            "physics number must be finite",
        ))
    }
}

fn check_extensions(
    fields: &BTreeMap<String, Value>,
    path: &str,
    known: &[&str],
) -> Result<(), PhysicsValidationError> {
    fn contains_number(value: &Value) -> bool {
        match value {
            Value::Number(_) => true,
            Value::Array(items) => items.iter().any(contains_number),
            Value::Object(items) => items.values().any(contains_number),
            _ => false,
        }
    }
    for (key, value) in fields {
        if known.contains(&key.as_str()) {
            return Err(error(
                "EXTENSION_KEY_COLLISION",
                format!("{path}.{key}"),
                "extension shadows typed field",
            ));
        }
        if contains_number(value) {
            return Err(error(
                "UNSUPPORTED_EXTENSION_NUMBER",
                format!("{path}.{key}"),
                "numeric unknown fields lack verified Framework encoding",
            ));
        }
    }
    Ok(())
}

fn validate(physics: &PhysicsDefinition, writer: bool) -> Result<(), PhysicsValidationError> {
    if physics.version != 3 {
        return Err(error(
            "INVALID_VERSION",
            "$.Version",
            "physics3 Version must be 3",
        ));
    }
    for (path, vector) in [
        (
            "$.Meta.EffectiveForces.Gravity",
            physics.meta.forces.gravity,
        ),
        ("$.Meta.EffectiveForces.Wind", physics.meta.forces.wind),
    ] {
        finite(vector.x, format!("{path}.X"))?;
        finite(vector.y, format!("{path}.Y"))?;
    }
    if let Some(fps) = physics.meta.fps {
        if !fps.is_finite() || fps < 0.0 {
            return Err(error(
                "INVALID_FPS",
                "$.Meta.Fps",
                "FPS must be nonnegative and finite",
            ));
        }
    }
    let mut ids = HashSet::new();
    for (index, setting) in physics.settings.iter().enumerate() {
        let path = format!("$.PhysicsSettings[{index}]");
        if setting.id.is_empty() || setting.id.contains('\0') || !ids.insert(setting.id.as_str()) {
            return Err(error(
                "INVALID_SETTING_ID",
                format!("{path}.Id"),
                "setting ID must be unique and nonempty",
            ));
        }
        if setting.vertices.is_empty() {
            return Err(error(
                "EMPTY_VERTICES",
                format!("{path}.Vertices"),
                "rig needs at least one vertex",
            ));
        }
        for (i, input) in setting.inputs.iter().enumerate() {
            let item = format!("{path}.Input[{i}]");
            if input.source.target != "Parameter"
                || input.source.id.is_empty()
                || input.source.id.contains('\0')
            {
                return Err(error(
                    "INVALID_INPUT_TARGET",
                    format!("{item}.Source"),
                    "input must refer to a Parameter ID",
                ));
            }
            finite(input.weight, format!("{item}.Weight"))?;
            if writer {
                check_extensions(
                    &input.source.extensions,
                    &format!("{item}.Source"),
                    &["Target", "Id"],
                )?;
                check_extensions(
                    &input.extensions,
                    &item,
                    &["Source", "Weight", "Type", "Reflect"],
                )?;
            }
        }
        for (i, output) in setting.outputs.iter().enumerate() {
            let item = format!("{path}.Output[{i}]");
            if output.destination.target != "Parameter"
                || output.destination.id.is_empty()
                || output.destination.id.contains('\0')
            {
                return Err(error(
                    "INVALID_OUTPUT_TARGET",
                    format!("{item}.Destination"),
                    "output must refer to a Parameter ID",
                ));
            }
            if output.vertex_index >= setting.vertices.len() {
                return Err(error(
                    "INVALID_VERTEX_INDEX",
                    format!("{item}.VertexIndex"),
                    "output vertex is absent",
                ));
            }
            finite(output.weight, format!("{item}.Weight"))?;
            finite(output.scale, format!("{item}.Scale"))?;
            if writer {
                check_extensions(
                    &output.destination.extensions,
                    &format!("{item}.Destination"),
                    &["Target", "Id"],
                )?;
                check_extensions(
                    &output.extensions,
                    &item,
                    &[
                        "Destination",
                        "VertexIndex",
                        "Scale",
                        "Weight",
                        "Type",
                        "Reflect",
                    ],
                )?;
            }
        }
        for (i, vertex) in setting.vertices.iter().enumerate() {
            let item = format!("{path}.Vertices[{i}]");
            for (label, value) in [
                ("Position.X", vertex.position.x),
                ("Position.Y", vertex.position.y),
                ("Mobility", vertex.mobility),
                ("Delay", vertex.delay),
                ("Acceleration", vertex.acceleration),
                ("Radius", vertex.radius),
            ] {
                finite(value, format!("{item}.{label}"))?;
            }
            if writer {
                check_extensions(
                    &vertex.extensions,
                    &item,
                    &["Position", "Mobility", "Delay", "Acceleration", "Radius"],
                )?;
            }
        }
        for (label, range) in [
            ("Position", setting.normalization.position),
            ("Angle", setting.normalization.angle),
        ] {
            for (field, value) in [
                ("Minimum", range.minimum),
                ("Default", range.default),
                ("Maximum", range.maximum),
            ] {
                finite(value, format!("{path}.Normalization.{label}.{field}"))?;
            }
            if range.minimum > range.default || range.default > range.maximum {
                return Err(error(
                    "INVALID_NORMALIZATION",
                    format!("{path}.Normalization.{label}"),
                    "normalization default must be within range",
                ));
            }
        }
        if writer {
            check_extensions(
                &setting.normalization.extensions,
                &format!("{path}.Normalization"),
                &["Position", "Angle"],
            )?;
            check_extensions(
                &setting.extensions,
                &path,
                &["Id", "Input", "Output", "Vertices", "Normalization"],
            )?;
        }
    }
    let dictionary_ids: HashSet<_> = physics
        .meta
        .dictionary
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    if dictionary_ids.len() != physics.meta.dictionary.len() || ids != dictionary_ids {
        return Err(error(
            "INVALID_PHYSICS_DICTIONARY",
            "$.Meta.PhysicsDictionary",
            "dictionary IDs must match settings",
        ));
    }
    if writer {
        check_extensions(
            &physics.extensions,
            "$",
            &["Version", "Meta", "PhysicsSettings"],
        )?;
        check_extensions(
            &physics.meta.extensions,
            "$.Meta",
            &[
                "PhysicsSettingCount",
                "TotalInputCount",
                "TotalOutputCount",
                "VertexCount",
                "EffectiveForces",
                "PhysicsDictionary",
                "Fps",
            ],
        )?;
        check_extensions(
            &physics.meta.forces.extensions,
            "$.Meta.EffectiveForces",
            &["Gravity", "Wind"],
        )?;
        for (index, item) in physics.meta.dictionary.iter().enumerate() {
            check_extensions(
                &item.extensions,
                &format!("$.Meta.PhysicsDictionary[{index}]"),
                &["Id", "Name"],
            )?;
        }
    }
    Ok(())
}

pub fn validate_physics_definition(
    physics: &PhysicsDefinition,
) -> Result<(), PhysicsValidationError> {
    validate(physics, false)
}

impl PhysicsDefinition {
    pub fn actual_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.settings.len(),
            self.settings
                .iter()
                .map(|setting| setting.inputs.len())
                .sum(),
            self.settings
                .iter()
                .map(|setting| setting.outputs.len())
                .sum(),
            self.settings
                .iter()
                .map(|setting| setting.vertices.len())
                .sum(),
        )
    }
    pub fn counts_match(&self) -> bool {
        self.actual_counts()
            == (
                self.meta.setting_count,
                self.meta.input_count,
                self.meta.output_count,
                self.meta.vertex_count,
            )
    }
}

pub fn validate_physics_export(physics: &PhysicsDefinition) -> Result<(), PhysicsValidationError> {
    validate(physics, true)
}
