//! Typed pose3 JSON without project IDs or file IO.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pose3 {
    #[serde(rename = "Type", default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(
        rename = "FadeInTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fade_in: Option<f32>,
    #[serde(rename = "Groups")]
    pub groups: Vec<Vec<Pose3Part>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pose3Part {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Link", default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<String>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("pose3 {code} at {path}: {message}")]
pub struct Pose3Error {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Pose3Error {
    Pose3Error {
        code,
        path: path.into(),
        message: message.into(),
    }
}

fn contains_number(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::Array(items) => items.iter().any(contains_number),
        Value::Object(items) => items.values().any(contains_number),
        _ => false,
    }
}

fn validate(pose: &Pose3, writer: bool) -> Result<(), Pose3Error> {
    if pose
        .file_type
        .as_deref()
        .is_some_and(|value| value != "Live2D Pose")
    {
        return Err(error("INVALID_TYPE", "$.Type", "Type must be Live2D Pose"));
    }
    if pose
        .fade_in
        .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err(error(
            "INVALID_FADE",
            "$.FadeInTime",
            "fade must be finite and nonnegative",
        ));
    }
    for (group_index, group) in pose.groups.iter().enumerate() {
        let mut ids = HashSet::new();
        for (index, part) in group.iter().enumerate() {
            let path = format!("$.Groups[{group_index}][{index}]");
            if part.id.is_empty() || part.id.contains('\0') || !ids.insert(part.id.as_str()) {
                return Err(error(
                    "INVALID_PART_ID",
                    format!("{path}.Id"),
                    "part ID is empty, repeated, or contains NUL",
                ));
            }
            if let Some(links) = &part.links {
                let mut linked = HashSet::new();
                for (link_index, id) in links.iter().enumerate() {
                    if id.is_empty() || id.contains('\0') || !linked.insert(id.as_str()) {
                        return Err(error(
                            "INVALID_LINK_ID",
                            format!("{path}.Link[{link_index}]"),
                            "link ID is empty, repeated, or contains NUL",
                        ));
                    }
                }
            }
            if writer {
                check_extensions(&part.extensions, &path, &["Id", "Link"])?;
            }
        }
    }
    if writer {
        check_extensions(&pose.extensions, "$", &["Type", "FadeInTime", "Groups"])?;
    }
    Ok(())
}

fn check_extensions(
    fields: &BTreeMap<String, Value>,
    path: &str,
    known: &[&str],
) -> Result<(), Pose3Error> {
    for (key, value) in fields {
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
                "numeric unknown fields lack verified Framework encoding",
            ));
        }
    }
    Ok(())
}

pub fn decode_pose3(text: &str) -> Result<Pose3, Pose3Error> {
    crate::cdi3::check_json_members(text).map_err(|failure| {
        error(
            "INVALID_JSON",
            failure.path().unwrap_or("$"),
            failure.to_string(),
        )
    })?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let pose: Pose3 = serde_path_to_error::deserialize(&mut deserializer).map_err(|failure| {
        error(
            "INVALID_FIELD",
            format!("$.{}", failure.path()),
            failure.inner().to_string(),
        )
    })?;
    deserializer
        .end()
        .map_err(|failure| error("INVALID_JSON", "$", failure.to_string()))?;
    validate(&pose, false)?;
    Ok(pose)
}

pub fn encode_pose3(pose: &Pose3) -> Result<String, Pose3Error> {
    validate(pose, true)?;
    let value = serde_json::to_value(pose)
        .map_err(|failure| error("SERIALIZATION", "$", failure.to_string()))?;
    let mut output = String::new();
    crate::motion3::write_value(&value, 0, &mut output);
    output.push('\n');
    Ok(output)
}
