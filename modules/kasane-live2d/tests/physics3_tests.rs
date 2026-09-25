use kasane_live2d::physics3::{decode_physics3, encode_physics3};
use serde_json::Value;

#[test]
fn physics3_roundtrip_recounts_and_preserves_fps_presence() {
    for source in [
        include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json"),
        include_str!("../../../tests/fixtures/animation_cpu/missing.physics3.json"),
        include_str!("../../../tests/fixtures/animation_cpu/zero.physics3.json"),
    ] {
        let physics = decode_physics3(source).unwrap();
        assert!(physics.counts_match());
        let encoded = encode_physics3(&physics).unwrap();
        let encoded_json: Value = serde_json::from_str(&encoded).unwrap();
        let source_json: Value = serde_json::from_str(source).unwrap();
        assert_eq!(
            encoded_json["Meta"].get("Fps").is_some(),
            source_json["Meta"].get("Fps").is_some()
        );
        assert_eq!(decode_physics3(&encoded).unwrap(), physics);
    }
}

#[test]
fn physics3_invalid_targets_and_unknown_numeric_extension_are_explicit() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json");
    let mut physics = decode_physics3(source).unwrap();
    physics.settings[0].outputs[0].vertex_index = 2;
    assert_eq!(
        encode_physics3(&physics).unwrap_err().code,
        "INVALID_VERTEX_INDEX"
    );
    let mut physics = decode_physics3(source).unwrap();
    physics.extensions.insert("Future".into(), Value::from(0.5));
    assert_eq!(
        encode_physics3(&physics).unwrap_err().code,
        "UNSUPPORTED_EXTENSION_NUMBER"
    );
}
