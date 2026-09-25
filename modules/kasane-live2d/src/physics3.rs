//! Live2D physics3 JSON boundary; domain data and simulation are format-independent consumers.
pub use kasane_core::physics::*;
pub use kasane_core::physics::{
    validate_physics_definition as validate_physics3, PhysicsDefinition as Physics3,
    PhysicsValidationError as Physics3Error,
};
fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Physics3Error {
    Physics3Error {
        code,
        path: path.into(),
        message: message.into(),
    }
}

pub fn decode_physics3(text: &str) -> Result<Physics3, Physics3Error> {
    crate::cdi3::check_json_members(text).map_err(|failure| {
        error(
            "INVALID_JSON",
            failure.path().unwrap_or("$"),
            failure.to_string(),
        )
    })?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let physics: Physics3 =
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
    validate_physics3(&physics)?;
    Ok(physics)
}

pub fn encode_physics3(physics: &Physics3) -> Result<String, Physics3Error> {
    validate_physics_export(physics)?;
    let mut wire = physics.clone();
    let (settings, inputs, outputs, vertices) = wire.actual_counts();
    wire.meta.setting_count = settings;
    wire.meta.input_count = inputs;
    wire.meta.output_count = outputs;
    wire.meta.vertex_count = vertices;
    let value = serde_json::to_value(wire)
        .map_err(|failure| error("SERIALIZATION", "$", failure.to_string()))?;
    let mut output = String::new();
    crate::motion3::write_value(&value, 0, &mut output);
    output.push('\n');
    Ok(output)
}
