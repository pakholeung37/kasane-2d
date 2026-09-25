//! Expression wire format. Playback and project UUID resolution live elsewhere.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Expression3 {
    #[serde(rename = "Type", default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
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
    #[serde(
        rename = "Parameters",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parameters: Option<Vec<Expression3Parameter>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpressionBlend {
    Add,
    Multiply,
    Overwrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expression3Parameter {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Value")]
    pub value: f32,
    #[serde(rename = "Blend", default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<ExpressionBlend>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("Expression {code} at {path}: {message}")]
pub struct ExpressionError {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

fn error(
    code: &'static str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ExpressionError {
    ExpressionError {
        code,
        path: path.into(),
        message: message.into(),
    }
}

fn check_text(value: &str, path: &str) -> Result<(), ExpressionError> {
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(error(
            "UNSUPPORTED_CONTROL_CHARACTER",
            path,
            "Framework cannot decode this character escape",
        ));
    }
    Ok(())
}

fn check_extension_value(value: &Value, path: &str, writer: bool) -> Result<(), ExpressionError> {
    match value {
        Value::Null | Value::Bool(_) => Ok(()),
        Value::Number(_) if writer => Err(error(
            "UNSUPPORTED_EXTENSION_NUMBER",
            path,
            "numeric extensions have no verified Framework encoding",
        )),
        Value::Number(_) => Ok(()),
        Value::String(text) if writer => check_text(text, path),
        Value::String(_) => Ok(()),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                check_extension_value(item, &format!("{path}[{index}]"), writer)?;
            }
            Ok(())
        }
        Value::Object(items) => {
            for (key, item) in items {
                let field = format!("{path}.{key}");
                if writer {
                    check_text(key, &field)?;
                }
                check_extension_value(item, &field, writer)?;
            }
            Ok(())
        }
    }
}

fn check_extensions(
    extensions: &BTreeMap<String, Value>,
    known: &[&str],
    path: &str,
    writer: bool,
) -> Result<(), ExpressionError> {
    for (key, value) in extensions {
        let field = format!("{path}.{key}");
        if known.contains(&key.as_str()) {
            return Err(error(
                "EXTENSION_KEY_COLLISION",
                field,
                "extension shadows a known field",
            ));
        }
        if writer {
            check_text(key, &field)?;
        }
        check_extension_value(value, &field, writer)?;
    }
    Ok(())
}

fn validate(expression: &Expression3, writer: bool) -> Result<(), ExpressionError> {
    if writer {
        if let Some(file_type) = &expression.file_type {
            check_text(file_type, "$.Type")?;
        }
    }
    for (field, value) in [
        ("FadeInTime", expression.fade_in),
        ("FadeOutTime", expression.fade_out),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value < 0.0 {
                return Err(error(
                    "INVALID_FADE",
                    format!("$.{field}"),
                    "fade time must be finite and nonnegative",
                ));
            }
        }
    }
    if let Some(parameters) = &expression.parameters {
        for (index, parameter) in parameters.iter().enumerate() {
            let path = format!("$.Parameters[{index}]");
            if parameter.id.is_empty() {
                return Err(error(
                    "EMPTY_ID",
                    format!("{path}.Id"),
                    "parameter ID is empty",
                ));
            }
            if writer {
                check_text(&parameter.id, &format!("{path}.Id"))?;
            }
            if !parameter.value.is_finite() {
                return Err(error(
                    "INVALID_VALUE",
                    format!("{path}.Value"),
                    "parameter value must be finite",
                ));
            }
            check_extensions(
                &parameter.extensions,
                &["Id", "Value", "Blend"],
                &path,
                writer,
            )?;
        }
    }
    check_extensions(
        &expression.extensions,
        &["Type", "FadeInTime", "FadeOutTime", "Parameters"],
        "$",
        writer,
    )
}

/// Decode known fields while retaining unknown fields. Missing fades and Blend
/// remain absent so the Framework defaults can be applied by the evaluator.
pub fn decode_exp3(text: &str) -> Result<Expression3, ExpressionError> {
    crate::cdi3::check_json_members(text).map_err(|failure| {
        error(
            "INVALID_JSON",
            failure.path().unwrap_or("$"),
            failure.to_string(),
        )
    })?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let expression: Expression3 =
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
    validate(&expression, false)?;
    Ok(expression)
}

/// Write decimal numbers without exponent notation and with a line break
/// after every number, matching the local Framework's numeric parser.
pub fn encode_exp3(expression: &Expression3) -> Result<String, ExpressionError> {
    validate(expression, true)?;
    let mut fields = Vec::new();
    if let Some(value) = &expression.file_type {
        fields.push(format!(
            "  \"Type\": {}",
            serde_json::to_string(value).expect("string serializes")
        ));
    }
    if let Some(value) = expression.fade_in {
        fields.push(format!("  \"FadeInTime\": {}", value));
    }
    if let Some(value) = expression.fade_out {
        fields.push(format!("  \"FadeOutTime\": {}", value));
    }
    if let Some(parameters) = &expression.parameters {
        let mut items = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let mut parts = vec![
                format!(
                    "      \"Id\": {}",
                    serde_json::to_string(&parameter.id).expect("string serializes")
                ),
                format!("      \"Value\": {}", parameter.value),
            ];
            if let Some(blend) = parameter.blend {
                parts.push(format!(
                    "      \"Blend\": {}",
                    serde_json::to_string(&blend).expect("enum serializes")
                ));
            }
            for (key, value) in &parameter.extensions {
                parts.push(format!(
                    "      {}: {}",
                    serde_json::to_string(key).expect("key serializes"),
                    serde_json::to_string(value).expect("extension serializes")
                ));
            }
            items.push(format!("    {{\n{}\n    }}", parts.join(",\n")));
        }
        fields.push(format!("  \"Parameters\": [\n{}\n  ]", items.join(",\n")));
    }
    for (key, value) in &expression.extensions {
        fields.push(format!(
            "  {}: {}",
            serde_json::to_string(key).expect("key serializes"),
            serde_json::to_string(value).expect("extension serializes")
        ));
    }
    Ok(format!("{{\n{}\n}}\n", fields.join(",\n")))
}
