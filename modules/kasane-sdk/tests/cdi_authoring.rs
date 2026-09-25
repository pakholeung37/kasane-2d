use kasane_core::document::{CdiCombinedSet, CdiParameterGroup, CdiParameterRef};
use kasane_core::{Canvas, Parameter, Vec2};
use kasane_sdk::{AuthoringSession, ObjectKind};
use serde_json::Value;

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000a01";
const FIRST: &str = "00000000-0000-4000-8000-000000000a02";
const SECOND: &str = "00000000-0000-4000-8000-000000000a03";
const THIRD: &str = "00000000-0000-4000-8000-000000000a04";
const GROUP: &str = "00000000-0000-4000-8000-000000000a05";
const SET: &str = "00000000-0000-4000-8000-000000000a06";

fn session() -> AuthoringSession {
    let mut session =
        AuthoringSession::new(DOCUMENT, Canvas::new(100.0, 100.0, Vec2::default(), 10.0)).unwrap();
    session
        .edit("parameters", None, |edit| {
            for (id, runtime_id) in [(FIRST, "ParamX"), (SECOND, "ParamY")] {
                edit.create_parameter(Parameter {
                    id: id.into(),
                    runtime_id: runtime_id.into(),
                    name: runtime_id.into(),
                    ..Default::default()
                })?;
            }
            Ok(())
        })
        .unwrap();
    session
}

#[test]
fn generated_cdi_group_edit_keeps_other_parameters_and_future_parameters() {
    let mut session = session();
    let eval_revision = session.evaluation_revision();
    session
        .edit("CDI", None, |edit| {
            edit.create_parameter_group(CdiParameterGroup {
                id: GROUP.into(),
                runtime_id: "Face".into(),
                name: "Face".into(),
                parent_id: None,
                extensions: Default::default(),
            })?;
            edit.set_parameter_group(FIRST, Some(GROUP))?;
            edit.set_parameter_display_name(FIRST, "角度".into())?;
            edit.set_combined_parameters(CdiCombinedSet {
                id: SET.into(),
                members: vec![
                    CdiParameterRef::Resolved {
                        parameter_id: FIRST.into(),
                    },
                    CdiParameterRef::Resolved {
                        parameter_id: SECOND.into(),
                    },
                ],
            })
        })
        .unwrap();
    assert_eq!(session.evaluation_revision(), eval_revision);
    let group_handle = session
        .handle(ObjectKind::CdiParameterGroup, GROUP)
        .unwrap();
    let set_handle = session.handle(ObjectKind::CdiCombinedSet, SET).unwrap();
    let output: Value = serde_json::from_str(&session.export_cdi3().unwrap()).unwrap();
    assert_eq!(output["Parameters"].as_array().unwrap().len(), 2);
    assert_eq!(output["Parameters"][0]["GroupId"], "Face");
    assert_eq!(output["Parameters"][1]["Id"], "ParamY");
    assert_eq!(
        output["CombinedParameters"][0],
        serde_json::json!(["ParamX", "ParamY"])
    );
    session.undo().unwrap();
    assert!(session.resolve_handle(&group_handle).is_err());
    session.redo().unwrap();
    assert!(session.resolve_handle(&group_handle).is_err());
    assert!(session.resolve_handle(&set_handle).is_err());
    assert_eq!(session.evaluation_revision(), eval_revision);

    session
        .edit("new parameter", None, |edit| {
            edit.create_parameter(Parameter {
                id: THIRD.into(),
                runtime_id: "ParamZ".into(),
                name: "Z".into(),
                ..Default::default()
            })
        })
        .unwrap();
    let output: Value = serde_json::from_str(&session.export_cdi3().unwrap()).unwrap();
    assert_eq!(output["Parameters"].as_array().unwrap().len(), 3);
    assert_eq!(output["Parameters"][2]["Id"], "ParamZ");
}

#[test]
fn imported_cdi_is_atomic_and_preserves_field_diagnostics() {
    let mut session = session();
    let input = r#"{"Version":3,"Parameters":[{"Id":"ParamX","GroupId":"","Name":"X"},{"Id":"Missing","GroupId":"","Name":"?"}]}"#;
    let (diagnostics, _) = session
        .edit("import CDI", None, |edit| edit.import_cdi3(input))
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path, "$.Parameters[1].Id");
    assert_eq!(
        session.export_cdi3().unwrap_err().field_path.as_deref(),
        Some("$.Parameters[1].Id")
    );
    let version = session.version();
    let bad = r#"{"Version":3,"Parameters":[{"Id":"ParamX","GroupId":"Unknown","Name":"X"}]}"#;
    assert!(session
        .edit("bad CDI", None, |edit| edit.import_cdi3(bad))
        .is_err());
    assert_eq!(session.version(), version);
}
