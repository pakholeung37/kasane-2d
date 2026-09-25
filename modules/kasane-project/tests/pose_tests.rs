use kasane_core::{Canvas, ChangeKind, Document, Part, Vec2};
use kasane_project::{decode_project, encode_project, export_pose3, import_pose3, DocumentSession};
use serde_json::Value;

const DOC: &str = "00000000-0000-4000-8000-000000000e01";
const PART_A: &str = "00000000-0000-4000-8000-000000000e02";
const PART_B: &str = "00000000-0000-4000-8000-000000000e03";
const POSE: &str = "00000000-0000-4000-8000-000000000e04";

fn document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    for (id, runtime) in [(PART_A, "Part0"), (PART_B, "Part1")] {
        assert!(doc
            .create_part(Part {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    doc
}

#[test]
fn pose3_import_roundtrips_uuid_refs_and_project_v5() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/minimal.pose3.json");
    let imported = import_pose3(&document(), POSE, source).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert!(reopened.validate_structure().is_empty());
    let encoded = export_pose3(&reopened).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(source).unwrap(),
        serde_json::from_str::<Value>(&encoded).unwrap()
    );
    assert_eq!(
        reopened.erase_object(PART_A).status.code,
        "OBJECT_REFERENCED"
    );
    assert!(reopened.erase_object(POSE).status.is_ok());
    assert!(reopened.erase_object(PART_A).status.is_ok());
}

#[test]
fn unresolved_and_opaque_pose_refs_block_strict_export() {
    let input = r#"{"Groups":[[{"Id":"Missing"}]]}"#;
    let imported = import_pose3(&document(), POSE, input).unwrap();
    assert_eq!(imported.diagnostics[0].path, "$.Groups[0][0].Id");
    assert_eq!(
        export_pose3(&imported.candidate).unwrap_err().code,
        "UNRESOLVED_PART"
    );
    let input = r#"{"Groups":[[{"Id":"Part0"}]],"Future":"keep"}"#;
    let mut candidate = import_pose3(&document(), POSE, input).unwrap().candidate;
    let mut part = candidate.get_part(PART_A).unwrap().clone();
    part.runtime_id = "Changed".into();
    assert!(candidate.replace_part(part).status.is_ok());
    assert_eq!(
        export_pose3(&candidate).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn model3_pose_package_roundtrip_and_corrupt_import_rollback() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut session = DocumentSession::new();
    let (result, _) =
        session.import_model3(&root.join("tests/fixtures/external_v50/model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    let part_id = session.document().part_order()[0].clone();
    let runtime_id = session
        .document()
        .get_part(&part_id)
        .unwrap()
        .runtime_id
        .clone();
    let source =
        serde_json::json!({"Type":"Live2D Pose","FadeInTime":0.5,"Groups":[[{"Id":runtime_id}]]})
            .to_string();
    let mut imported = import_pose3(session.document(), POSE, &source).unwrap();
    assert!(imported
        .candidate
        .set_missing_attachments(Vec::new())
        .status
        .is_ok());
    assert!(session
        .publish_authoring_candidate(imported.candidate, ChangeKind::Metadata, false)
        .unwrap());
    let temporary = std::env::temp_dir().join(format!("kasane-pose-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    let result = session.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    let model3: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["Pose"], "model.pose3.json");
    assert!(package.join("model.pose3.json").is_file());
    let mut reopened = DocumentSession::new();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(
        serde_json::from_str::<Value>(&export_pose3(reopened.document()).unwrap().unwrap())
            .unwrap(),
        serde_json::from_str::<Value>(&source).unwrap()
    );
    let revision = reopened.document().revision();
    std::fs::write(package.join("model.pose3.json"), "{broken").unwrap();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert_eq!(reopened.document().revision(), revision);
    std::fs::remove_dir_all(temporary).unwrap();
}
