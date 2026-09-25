use kasane_live2d::exp3::{
    decode_exp3, encode_exp3, Expression3, Expression3Parameter, ExpressionBlend,
};
use serde_json::json;

#[test]
fn expression_wire_preserves_defaults_and_extensions() {
    let input = r#"{"Type":"Live2D Expression","Parameters":[{"Id":"ParamX","Value":0.75},{"Id":"ParamY","Value":-1,"Blend":"Overwrite","Note":"眉"}],"Future":{"Enabled":true}}"#;
    let decoded = decode_exp3(input).unwrap();
    assert_eq!(decoded.fade_in, None);
    assert_eq!(decoded.parameters.as_ref().unwrap()[0].blend, None);
    let output = encode_exp3(&decoded).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&output).unwrap(),
        serde_json::from_str::<serde_json::Value>(input).unwrap()
    );
    assert_eq!(decode_exp3(&output).unwrap(), decoded);
    assert!(output.contains("\"Value\": 0.75\n"));
}

#[test]
fn expression_writer_expands_exponents_and_rejects_unsupported_extensions() {
    let mut expression = Expression3 {
        file_type: Some("Live2D Expression".into()),
        fade_in: Some(1e-8),
        fade_out: Some(0.0),
        parameters: Some(vec![Expression3Parameter {
            id: "ParamX".into(),
            value: 1e-30,
            blend: Some(ExpressionBlend::Multiply),
            extensions: Default::default(),
        }]),
        extensions: Default::default(),
    };
    let output = encode_exp3(&expression).unwrap();
    assert!(!output.contains("e-") && !output.contains("E-"));
    assert_eq!(decode_exp3(&output).unwrap(), expression);
    expression
        .extensions
        .insert("Future".into(), json!({"Gain": 0.5}));
    assert_eq!(encode_exp3(&expression).unwrap_err().path, "$.Future.Gain");
    let imported = decode_exp3(r#"{"Future":{"Gain":0.5}}"#).unwrap();
    assert_eq!(imported.extensions["Future"]["Gain"], 0.5);
}

#[test]
fn expression_wire_rejects_duplicate_keys_invalid_modes_and_nonfinite_values() {
    assert_eq!(
        decode_exp3(r#"{"FadeInTime":1,"FadeInTime":2}"#)
            .unwrap_err()
            .code,
        "INVALID_JSON"
    );
    assert_eq!(
        decode_exp3(r#"{"Parameters":[{"Id":"P","Value":1,"Blend":"Other"}]}"#)
            .unwrap_err()
            .code,
        "INVALID_FIELD"
    );
    let expression = Expression3 {
        fade_in: Some(f32::NAN),
        ..Default::default()
    };
    assert_eq!(encode_exp3(&expression).unwrap_err().path, "$.FadeInTime");
}
