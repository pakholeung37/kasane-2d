use kasane_live2d::motion3::{decode_motion3, encode_motion3, MotionSegment};

const TYPED: &str = r#"{
  "Version": 3,
  "Meta": {"Duration":1,"Fps":30,"Loop":false,"AreBeziersRestricted":false,
    "CurveCount":0,"TotalSegmentCount":0,"TotalPointCount":0,
    "UserDataCount":0,"TotalUserDataSize":0},
  "Curves": [{"Target":"Parameter","Id":"ParamX","Segments":[
    0,0,0,0.25,1,1,0.3,1.2,0.45,1.1,0.5,0.5,2,0.75,0.1,3,1,0.9
  ]}],
  "UserData": [{"Time":0.75,"Value":"你好"}]
}"#;

#[test]
fn typed_segments_and_utf8_event_counts_roundtrip() {
    let motion = decode_motion3(TYPED).unwrap();
    assert!(!motion.counts_match());
    assert_eq!(motion.actual_counts(), (1, 4, 7, 1, 6));
    assert!(matches!(
        motion.curves[0].segments[1],
        MotionSegment::Bezier { .. }
    ));
    assert!(matches!(
        motion.curves[0].segments[2],
        MotionSegment::Stepped { .. }
    ));
    assert!(matches!(
        motion.curves[0].segments[3],
        MotionSegment::InverseStepped { .. }
    ));
    let encoded = encode_motion3(&motion).unwrap();
    let decoded = decode_motion3(&encoded).unwrap();
    assert!(decoded.counts_match());
    assert_eq!(decoded.curves, motion.curves);
    assert_eq!(decoded.user_data, motion.user_data);
}

#[test]
fn rejects_broken_segment_and_preserves_small_decimal_without_exponent() {
    let invalid = TYPED.replace("3,1,0.9", "7,1,0.9");
    let error = decode_motion3(&invalid).unwrap_err();
    assert_eq!(error.code, "INVALID_SEGMENT_TYPE");
    assert_eq!(error.path, "$.Curves[0].Segments[15]");
    let mut motion = decode_motion3(TYPED).unwrap();
    motion.curves[0].initial.value = 0.00000001;
    let encoded = encode_motion3(&motion).unwrap();
    assert!(!encoded.contains("e-"));
    assert_eq!(
        decode_motion3(&encoded).unwrap().curves[0].initial.value,
        0.00000001
    );
}
