use kasane_core::document::{MotionGroup, MotionRegistration};
use kasane_core::{Canvas, ChangeKind, Document, Parameter, Vec2};
use kasane_project::{
    decode_project, encode_project, export_motion3, import_motion3, DocumentSession,
};
use serde_json::Value;

const DOC: &str = "00000000-0000-4000-8000-000000000c01";
const PARAM: &str = "00000000-0000-4000-8000-000000000c02";
const MOTION: &str = "00000000-0000-4000-8000-000000000c03";

fn document() -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    assert!(document
        .create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    document
}

#[test]
fn motion_import_roundtrips_segments_events_and_v5_project() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/typed.motion3.json");
    let imported = import_motion3(&document(), MOTION, "Idle", source).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert_eq!(reopened.motion_order(), &[MOTION]);
    assert!(reopened.validate_structure().is_empty());
    let exported = export_motion3(&reopened, MOTION).unwrap();
    let output: Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(output["Meta"]["TotalUserDataSize"], 6);
    let original: Value = serde_json::from_str(source).unwrap();
    for (actual, expected) in output["Curves"][0]["Segments"]
        .as_array()
        .unwrap()
        .iter()
        .zip(original["Curves"][0]["Segments"].as_array().unwrap())
    {
        assert!((actual.as_f64().unwrap() - expected.as_f64().unwrap()).abs() < 0.000001);
    }
    assert_eq!(output["UserData"][0]["Value"], "你好");
    let track_id = reopened.get_motion(MOTION).unwrap().tracks[0].id.clone();
    let changed = reopened.get_motion(MOTION).unwrap().clone();
    assert_eq!(changed.tracks[0].id, track_id);
    assert_eq!(
        reopened.erase_object(PARAM).status.code,
        "OBJECT_REFERENCED"
    );
    assert!(reopened.erase_object(MOTION).status.is_ok());
    assert!(reopened.erase_object(PARAM).status.is_ok());
}

#[test]
fn virtual_motion_targets_roundtrip_and_opaque_namespace_changes_block_export() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/minimal.motion3.json");
    let imported = import_motion3(&document(), MOTION, "Mixed", source).unwrap();
    assert_eq!(imported.diagnostics.len(), 2);
    let exported: Value =
        serde_json::from_str(&export_motion3(&imported.candidate, MOTION).unwrap()).unwrap();
    assert_eq!(exported["Curves"][2]["Id"], "Part0");

    let source = r#"{"Version":3,"Meta":{"Duration":1,"Fps":30,"Loop":false,"AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Parameter","Id":"ParamX","Segments":[0,0,0,1,1]}],"Future":{"Label":"保留"}}"#;
    let mut candidate = import_motion3(&document(), MOTION, "Idle", source)
        .unwrap()
        .candidate;
    assert!(export_motion3(&candidate, MOTION).is_ok());
    let mut parameter = candidate.get_parameter(PARAM).unwrap().clone();
    parameter.runtime_id = "Changed".into();
    assert!(candidate.replace_parameter(parameter).status.is_ok());
    assert_eq!(
        export_motion3(&candidate, MOTION).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn motion_group_registration_is_separate_and_roundtrips() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/typed.motion3.json");
    let mut candidate = import_motion3(&document(), MOTION, "Idle", source)
        .unwrap()
        .candidate;
    let registration = MotionRegistration {
        clip_id: MOTION.into(),
        fade_in: Some(0.5),
        fade_out: Some(0.25),
        sound: None,
        extensions: Default::default(),
    };
    assert!(candidate
        .set_motion_groups(vec![
            MotionGroup {
                name: "Idle".into(),
                entries: vec![registration.clone()]
            },
            MotionGroup {
                name: "".into(),
                entries: vec![registration]
            },
        ])
        .status
        .is_ok());
    let reopened = decode_project(&encode_project(&candidate).unwrap()).unwrap();
    assert_eq!(reopened.motion_order().len(), 1);
    assert_eq!(reopened.motion_groups().len(), 2);
    assert_eq!(reopened.motion_groups()[1].name, "");
    assert_eq!(reopened.motion_groups()[1].entries[0].fade_out, Some(0.25));
    assert!(reopened.validate_structure().is_empty());
}

#[test]
fn model3_motion_package_preserves_shared_file_registrations_and_atomic_import() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut session = DocumentSession::new();
    let (result, _) =
        session.import_model3(&root.join("tests/fixtures/external_v50/model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "MISSING_MOTION_ATTACHMENT"));
    let runtime_id = session
        .document()
        .get_parameter(&session.document().parameter_order()[0])
        .unwrap()
        .runtime_id
        .clone();
    let source = serde_json::json!({
        "Version":3,"Meta":{"Duration":1,"Fps":30,"Loop":false,"AreBeziersRestricted":true,
            "CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,"UserDataCount":0,"TotalUserDataSize":0},
        "Curves":[{"Target":"Parameter","Id":runtime_id,"Segments":[0,0,0,1,1]}],"UserData":[]
    }).to_string();
    let mut candidate = import_motion3(session.document(), MOTION, "Idle", &source)
        .unwrap()
        .candidate;
    let entry = MotionRegistration {
        clip_id: MOTION.into(),
        fade_in: Some(0.4),
        fade_out: None,
        sound: None,
        extensions: Default::default(),
    };
    assert!(candidate
        .set_motion_groups(vec![
            MotionGroup {
                name: "Idle".into(),
                entries: vec![entry.clone()]
            },
            MotionGroup {
                name: "TapBody".into(),
                entries: vec![entry]
            },
        ])
        .status
        .is_ok());
    assert!(candidate.set_missing_attachments(Vec::new()).status.is_ok());
    assert!(session
        .publish_authoring_candidate(candidate, ChangeKind::Metadata, false)
        .unwrap());
    let temporary =
        std::env::temp_dir().join(format!("kasane-motion-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    let result = session.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    let model3: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert!(
        (model3["FileReferences"]["Motions"]["Idle"][0]["FadeInTime"]
            .as_f64()
            .unwrap()
            - 0.4)
            .abs()
            < 0.000001
    );
    assert_eq!(
        model3["FileReferences"]["Motions"]["Idle"][0]["File"],
        model3["FileReferences"]["Motions"]["TapBody"][0]["File"]
    );
    let relative = model3["FileReferences"]["Motions"]["Idle"][0]["File"]
        .as_str()
        .unwrap();
    assert_eq!(relative, "motions/Idle.motion3.json");
    let mut reopened = DocumentSession::new();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(reopened.document().motion_order().len(), 1);
    assert_eq!(reopened.document().motion_groups().len(), 2);
    let revision = reopened.document().revision();
    std::fs::write(package.join(relative), "{broken").unwrap();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert_eq!(reopened.document().revision(), revision);
    std::fs::remove_dir_all(temporary).unwrap();
}
