use kasane_core::{Canvas, Parameter, Vec2};
use kasane_sdk::{AuthoringSession, ObjectKind};
use serde_json::Value;

const DOC: &str = "00000000-0000-4000-8000-000000000c01";
const PARAM: &str = "00000000-0000-4000-8000-000000000c02";
const EXP: &str = "00000000-0000-4000-8000-000000000c03";

fn session() -> AuthoringSession {
    let mut session =
        AuthoringSession::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0)).unwrap();
    session
        .edit("parameter", None, |edit| {
            edit.create_parameter(Parameter {
                id: PARAM.into(),
                runtime_id: "ParamX".into(),
                name: "X".into(),
                ..Default::default()
            })
        })
        .unwrap();
    session
}

#[test]
fn expression_import_is_undoable_and_saved_with_v5_project() {
    let mut session = session();
    let evaluation_revision = session.evaluation_revision();
    let input =
        r#"{"Type":"Live2D Expression","Parameters":[{"Id":"ParamX","Value":0.5,"Blend":"Add"}]}"#;
    let (diagnostics, _) = session
        .edit("expression", None, |edit| {
            edit.import_expression3(EXP, "Smile", input)
        })
        .unwrap();
    assert!(diagnostics.is_empty());
    assert!(session.evaluation_revision() > evaluation_revision);
    let handle = session.handle(ObjectKind::Expression, EXP).unwrap();
    let output: Value = serde_json::from_str(&session.export_expression3(EXP).unwrap()).unwrap();
    assert_eq!(output["Parameters"][0]["Id"], "ParamX");
    session.undo().unwrap();
    assert!(session.resolve_handle(&handle).is_err());
    session.redo().unwrap();
    assert!(session.resolve_handle(&handle).is_err());
    let root = std::env::temp_dir().join(format!("kasane-expression-sdk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let project = root.join("project");
    session.save_project(&project, None).unwrap();
    let mut reopened =
        AuthoringSession::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0)).unwrap();
    reopened.open_project(&project, None).unwrap();
    assert_eq!(reopened.expression(EXP).unwrap().name, "Smile");
    assert_eq!(
        serde_json::from_str::<Value>(&reopened.export_expression3(EXP).unwrap()).unwrap(),
        output
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_expression_import_keeps_edit_atomic() {
    let mut session = session();
    let before = session.version();
    let invalid = r#"{"Parameters":[{"Id":"ParamX","Value":0.5,"Blend":"Unknown"}]}"#;
    let error = session
        .edit("bad expression", None, |edit| {
            edit.import_expression3(EXP, "Smile", invalid)
        })
        .unwrap_err();
    assert_eq!(error.field_path.as_deref(), Some("$.Parameters[0].Blend"));
    assert_eq!(session.version(), before);
    assert!(session.expression_ids().is_empty());
    let unresolved = r#"{"Parameters":[{"Id":"Missing","Value":0.5}]}"#;
    let (diagnostics, _) = session
        .edit("repairable", None, |edit| {
            edit.import_expression3(EXP, "Smile", unresolved)
        })
        .unwrap();
    assert_eq!(diagnostics[0].path, "$.Parameters[0].Id");
    assert_eq!(
        session.export_expression3(EXP).unwrap_err().code.as_ref(),
        "UNRESOLVED_PARAMETER"
    );
}
