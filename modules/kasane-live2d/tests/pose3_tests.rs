use kasane_live2d::pose3::{decode_pose3, encode_pose3};
use serde_json::Value;

#[test]
fn pose3_roundtrip_preserves_groups_links_and_missing_fade() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/minimal.pose3.json");
    let mut pose = decode_pose3(source).unwrap();
    assert_eq!(pose.groups[0].len(), 2);
    pose.groups[0][0].links = Some(vec!["PartLinked".into()]);
    let encoded = encode_pose3(&pose).unwrap();
    let reopened = decode_pose3(&encoded).unwrap();
    assert_eq!(reopened, pose);
    assert_eq!(
        serde_json::from_str::<Value>(&encoded).unwrap()["Groups"][0][0]["Link"][0],
        "PartLinked"
    );
}

#[test]
fn pose3_rejects_duplicate_fields_and_unsafe_numeric_extensions() {
    let source = r#"{"Groups":[],"Groups":[]}"#;
    assert_eq!(decode_pose3(source).unwrap_err().code, "INVALID_JSON");
    let mut pose = decode_pose3(r#"{"Groups":[],"Future":0.25}"#).unwrap();
    assert_eq!(
        encode_pose3(&pose).unwrap_err().code,
        "UNSUPPORTED_EXTENSION_NUMBER"
    );
    pose.extensions.remove("Future");
    assert!(encode_pose3(&pose).is_ok());
}
