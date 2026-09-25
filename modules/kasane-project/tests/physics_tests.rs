use kasane_core::{Canvas, ChangeKind, Document, Parameter, Vec2};
use kasane_project::{
    decode_project, encode_project, export_physics3, import_physics3, DocumentSession,
};

const DOC: &str = "00000000-0000-4000-8000-000000000f11";
const X: &str = "00000000-0000-4000-8000-000000000f12";
const Y: &str = "00000000-0000-4000-8000-000000000f13";
const PHYSICS: &str = "00000000-0000-4000-8000-000000000f14";

fn document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    for (id, runtime) in [(X, "ParamX"), (Y, "ParamY")] {
        assert!(doc
            .create_parameter(Parameter {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                ..Default::default()
            })
            .status
            .is_ok());
    }
    doc
}

#[test]
fn physics3_roundtrip_project_and_reference_protection() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json");
    let imported = import_physics3(&document(), PHYSICS, source).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert!(reopened.validate_structure().is_empty());
    let actual: serde_json::Value =
        serde_json::from_str(&export_physics3(&reopened).unwrap().unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(source).unwrap();
    assert_eq!(
        actual["Meta"]["PhysicsSettingCount"],
        expected["Meta"]["PhysicsSettingCount"]
    );
    assert_eq!(
        actual["PhysicsSettings"][0]["Input"][0]["Source"]["Id"],
        "ParamX"
    );
    assert_eq!(reopened.erase_object(X).status.code, "OBJECT_REFERENCED");
    assert!(reopened.erase_object(PHYSICS).status.is_ok());
    assert!(reopened.erase_object(X).status.is_ok());
}

#[test]
fn physics3_unresolved_and_opaque_edits_block_strict_export() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json");
    let mut missing = document();
    assert!(missing.erase_object(Y).status.is_ok());
    let imported = import_physics3(&missing, PHYSICS, source).unwrap();
    assert_eq!(imported.diagnostics[0].code, "UNRESOLVED_PARAMETER");
    assert_eq!(
        export_physics3(&imported.candidate).unwrap_err().code,
        "UNRESOLVED_PARAMETER"
    );

    let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
    value["Future"] = "keep".into();
    let mut candidate = import_physics3(&document(), PHYSICS, &value.to_string())
        .unwrap()
        .candidate;
    let mut parameter = candidate.get_parameter(X).unwrap().clone();
    parameter.runtime_id = "Renamed".into();
    assert!(candidate.replace_parameter(parameter).status.is_ok());
    assert_eq!(
        export_physics3(&candidate).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn model3_physics_package_roundtrip_and_bad_attachment_rollback() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut session = DocumentSession::new();
    let (result, _) =
        session.import_model3(&root.join("tests/fixtures/external_v50/model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    let source = include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json");
    let mut imported = import_physics3(session.document(), PHYSICS, source).unwrap();
    assert!(imported.diagnostics.is_empty());
    assert!(imported
        .candidate
        .set_missing_attachments(Vec::new())
        .status
        .is_ok());
    assert!(session
        .publish_authoring_candidate(imported.candidate, ChangeKind::Metadata, false)
        .unwrap());
    let temporary =
        std::env::temp_dir().join(format!("kasane-physics-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    let result = session.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    let model3: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["Physics"], "model.physics3.json");
    assert!(package.join("model.physics3.json").is_file());
    let mut reopened = DocumentSession::new();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(reopened.document().physics().is_some());
    let revision = reopened.document().revision();
    std::fs::write(package.join("model.physics3.json"), "{broken").unwrap();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert_eq!(reopened.document().revision(), revision);
    std::fs::remove_dir_all(temporary).unwrap();
}
