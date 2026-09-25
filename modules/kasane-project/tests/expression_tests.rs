use kasane_core::{Canvas, ChangeKind, Document, Parameter, Vec2};
use kasane_project::{
    decode_project, encode_project, export_expression3, import_expression3, DocumentSession,
};
use serde_json::{json, Value};

const DOC: &str = "00000000-0000-4000-8000-000000000b01";
const PARAM: &str = "00000000-0000-4000-8000-000000000b02";
const EXP: &str = "00000000-0000-4000-8000-000000000b03";

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
fn expression_import_roundtrips_v5_and_runtime_id_renames() {
    let input = r#"{"Type":"Live2D Expression","FadeInTime":0.25,"Parameters":[{"Id":"ParamX","Value":0.75,"Blend":"Overwrite"}]}"#;
    let imported = import_expression3(&document(), EXP, "Smile", input).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert_eq!(reopened.expression_order(), &[EXP]);
    let output: Value = serde_json::from_str(&export_expression3(&reopened, EXP).unwrap()).unwrap();
    assert_eq!(output, serde_json::from_str::<Value>(input).unwrap());
    let mut parameter = reopened.get_parameter(PARAM).unwrap().clone();
    parameter.runtime_id = "ParamChanged".into();
    assert!(reopened.replace_parameter(parameter).status.is_ok());
    let output: Value = serde_json::from_str(&export_expression3(&reopened, EXP).unwrap()).unwrap();
    assert_eq!(output["Parameters"][0]["Id"], "ParamChanged");
    assert_eq!(
        reopened.erase_object(PARAM).status.code,
        "OBJECT_REFERENCED"
    );
}

#[test]
fn unresolved_and_opaque_expression_targets_have_precise_export_errors() {
    let input = r#"{"Parameters":[{"Id":"Missing","Value":0.5}]}"#;
    let imported = import_expression3(&document(), EXP, "Missing", input).unwrap();
    assert_eq!(imported.diagnostics[0].path, "$.Parameters[0].Id");
    let reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert_eq!(
        export_expression3(&reopened, EXP).unwrap_err().code,
        "UNRESOLVED_PARAMETER"
    );
    let input = r#"{"Parameters":[{"Id":"ParamX","Value":0.5}],"Future":{"Label":"保留"}}"#;
    let imported = import_expression3(&document(), EXP, "Smile", input)
        .unwrap()
        .candidate;
    let mut renamed = decode_project(&encode_project(&imported).unwrap()).unwrap();
    let mut parameter = renamed.get_parameter(PARAM).unwrap().clone();
    parameter.runtime_id = "Changed".into();
    assert!(renamed.replace_parameter(parameter).status.is_ok());
    assert_eq!(
        export_expression3(&renamed, EXP).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );

    let source = r#"{"Parameters":[{"Id":"ParamX","Value":0.5}],"Future":{"Label":"保留"}}"#;
    let mut candidate = import_expression3(&document(), EXP, "Smile", source)
        .unwrap()
        .candidate;
    let mut edited = candidate.get_expression(EXP).unwrap().clone();
    edited.fade_in = Some(0.25);
    assert!(candidate.replace_expression(edited).status.is_ok());
    assert_eq!(
        export_expression3(&candidate, EXP).unwrap_err().code,
        "OPAQUE_CONTENT_CHANGED"
    );
    let reimported = import_expression3(&candidate, EXP, "Smile", source).unwrap();
    assert!(export_expression3(&reimported.candidate, EXP).is_ok());
}

#[test]
fn model3_expression_package_import_is_atomic() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/external_v50/model.model3.json");
    let mut session = DocumentSession::new();
    let (result, _) = session.import_model3(&source);
    assert!(result.status.is_ok(), "{result:?}");
    let runtime_id = session
        .document()
        .get_parameter(&session.document().parameter_order()[0])
        .unwrap()
        .runtime_id
        .clone();
    let expression = json!({"Type":"Live2D Expression","Parameters":[{
        "Id":runtime_id,"Value":0.25,"Blend":"Add"
    }]})
    .to_string();
    let mut imported = import_expression3(session.document(), EXP, "Smile", &expression).unwrap();
    assert!(imported
        .candidate
        .set_missing_attachments(Vec::new())
        .status
        .is_ok());
    assert!(session
        .publish_authoring_candidate(imported.candidate, ChangeKind::Metadata, false)
        .unwrap());
    let temporary =
        std::env::temp_dir().join(format!("kasane-expression-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    let result = session.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    let model3: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["Expressions"][0]["Name"], "Smile");
    let relative = model3["FileReferences"]["Expressions"][0]["File"]
        .as_str()
        .unwrap();
    assert!(package.join(relative).is_file());
    let mut reopened = DocumentSession::new();
    let (result, report) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(!report
        .unwrap()
        .unimported_attachments
        .iter()
        .any(|item| item.starts_with("Expressions:")));
    assert_eq!(reopened.document().expression_order().len(), 1);
    let id = &reopened.document().expression_order()[0];
    assert_eq!(
        reopened.document().get_expression(id).unwrap().name,
        "Smile"
    );
    let before = reopened.document().revision();
    std::fs::write(package.join(relative), "{broken").unwrap();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert_eq!(reopened.document().revision(), before);
    assert_eq!(reopened.document().expression_order().len(), 1);
    std::fs::remove_dir_all(temporary).unwrap();
}
