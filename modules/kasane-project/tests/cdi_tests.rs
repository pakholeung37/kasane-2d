use kasane_core::document::{CdiParameterEntry, DisplayInfo};
use kasane_core::ChangeKind;
use kasane_core::{Canvas, Document, Parameter, Part, Vec2};
use kasane_project::{
    decode_project, encode_project, export_cdi3, import_cdi3, DocumentSession, DocumentStore,
};
use serde_json::{json, Value};

const DOC: &str = "00000000-0000-4000-8000-000000000901";
const PARAM_X: &str = "00000000-0000-4000-8000-000000000902";
const PARAM_Y: &str = "00000000-0000-4000-8000-000000000903";
const PART: &str = "00000000-0000-4000-8000-000000000904";

fn document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    for (id, runtime_id) in [(PARAM_X, "ParamX"), (PARAM_Y, "ParamY")] {
        assert!(doc
            .create_parameter(Parameter {
                id: id.into(),
                runtime_id: runtime_id.into(),
                name: runtime_id.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    assert!(doc
        .create_part(Part {
            id: PART.into(),
            runtime_id: "PartA".into(),
            name: "Part A".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    doc
}

#[test]
fn model3_package_carries_cdi_and_failed_cdi_import_keeps_session() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/external_v50/model.model3.json");
    let mut session = DocumentSession::new();
    let (result, _) = session.import_model3(&source);
    assert!(result.status.is_ok(), "{result:?}");
    let parameter_id = session.document().parameter_order()[0].clone();
    let runtime_id = session
        .document()
        .get_parameter(&parameter_id)
        .unwrap()
        .runtime_id
        .clone();
    let cdi = json!({"Version":3,"Parameters":[{
        "Id":runtime_id,"GroupId":"","Name":"眉の角度"
    }]})
    .to_string();
    let mut imported = import_cdi3(session.document(), &cdi).unwrap();
    assert!(imported
        .candidate
        .set_missing_attachments(Vec::new())
        .status
        .is_ok());
    assert!(session
        .publish_authoring_candidate(imported.candidate, ChangeKind::Metadata, false)
        .unwrap());
    let temporary =
        std::env::temp_dir().join(format!("kasane-cdi-model3-package-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    let result = session.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    let model3: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["DisplayInfo"], "model.cdi3.json");
    let exported: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.cdi3.json")).unwrap()).unwrap();
    assert_eq!(exported["Parameters"][0]["Name"], "眉の角度");
    let mut reopened = DocumentSession::new();
    let (result, report) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(!report
        .unwrap()
        .unimported_attachments
        .iter()
        .any(|entry| entry.starts_with("DisplayInfo:")));
    assert_eq!(
        reopened
            .document()
            .get_parameter(&parameter_id)
            .unwrap()
            .name,
        "眉の角度"
    );

    std::fs::write(package.join("model.cdi3.json"), "{broken").unwrap();
    let before = reopened.document().checkpoint();
    let (result, _) = reopened.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert_eq!(
        reopened.document().checkpoint().estimated_bytes(),
        before.estimated_bytes()
    );
    assert_eq!(
        reopened
            .document()
            .get_parameter(&parameter_id)
            .unwrap()
            .name,
        "眉の角度"
    );
    std::fs::remove_dir_all(temporary).unwrap();
}

const CDI: &str = r#"{
  "Version": 3,
  "Parameters": [
    {"Id":"ParamX","GroupId":"Face","Name":"角度 X","Hint":"保持"},
    {"Id":"ParamY","GroupId":"Face","Name":"角度 X"}
  ],
  "ParameterGroups": [{"Id":"Face","GroupId":"","Name":"顔"}],
  "Parts": [{"Id":"PartA","Name":"头发"}],
  "CombinedParameters": [["ParamX","ParamY"]],
  "Future": {"Label":"保留","Enabled":true}
}"#;

#[test]
fn candidate_import_export_and_reimport_preserve_identity_and_extensions() {
    let source = document();
    let imported = import_cdi3(&source, CDI).unwrap();
    assert!(imported.diagnostics.is_empty());
    assert_eq!(source.get_parameter(PARAM_X).unwrap().name, "ParamX");
    assert_eq!(
        imported.candidate.get_parameter(PARAM_X).unwrap().name,
        "角度 X"
    );
    assert_eq!(
        imported.candidate.get_parameter(PARAM_Y).unwrap().name,
        "角度 X"
    );
    let reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert!(reopened.same_content(&imported.candidate));
    let wire: Value = serde_json::from_str(&export_cdi3(&reopened).unwrap()).unwrap();
    let expected: Value = serde_json::from_str(CDI).unwrap();
    assert_eq!(wire, expected);
    let again = import_cdi3(&reopened, CDI).unwrap();
    assert!(again.candidate.same_content(&reopened));
    assert_eq!(
        again
            .candidate
            .display_info()
            .parameter_groups
            .as_ref()
            .unwrap()[0]
            .id,
        reopened.display_info().parameter_groups.as_ref().unwrap()[0].id
    );
    assert_eq!(
        again
            .candidate
            .display_info()
            .combined_parameters
            .as_ref()
            .unwrap()[0]
            .id,
        reopened
            .display_info()
            .combined_parameters
            .as_ref()
            .unwrap()[0]
            .id
    );
}

#[test]
fn nonempty_cdi_survives_store_save_and_session_open() {
    let candidate = import_cdi3(&document(), CDI).unwrap().candidate;
    let root = std::env::temp_dir().join(format!("kasane-cdi-p1b-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let target = root.join("project");
    let (result, snapshot) = DocumentStore::new().save(&candidate, &root, &target, None);
    assert!(result.status.is_ok(), "{result:?}");
    let manifest = snapshot.unwrap().manifest;
    let saved: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(saved["format_version"], 6);
    assert!(saved["document"]["display_info"]["extensions"]["Future"].is_object());
    let mut session = DocumentSession::new();
    let opened = session.open(&manifest);
    assert!(opened.status.is_ok(), "{opened:?}");
    assert!(session.document().same_content(&candidate));
    assert_eq!(
        serde_json::from_str::<Value>(&export_cdi3(session.document()).unwrap()).unwrap(),
        serde_json::from_str::<Value>(CDI).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unresolved_targets_survive_project_save_but_block_strict_cdi_export() {
    let source = document();
    let input = r#"{"Version":3,"Parameters":[{"Id":"Missing","GroupId":"","Name":"未找到"}],"CombinedParameters":[["ParamX","Missing"]]}"#;
    let imported = import_cdi3(&source, input).unwrap();
    assert_eq!(imported.diagnostics.len(), 2);
    assert!(matches!(
        imported
            .candidate
            .display_info()
            .parameters
            .as_ref()
            .unwrap()[0],
        CdiParameterEntry::Unresolved { .. }
    ));
    let reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert_eq!(reopened.display_info(), imported.candidate.display_info());
    let error = export_cdi3(&reopened).unwrap_err();
    assert_eq!(error.code, "UNRESOLVED_PARAMETER");
    assert_eq!(error.path, "$.Parameters[0].Id");
}

#[test]
fn opaque_extensions_keep_full_namespace_provenance_and_writer_diagnostics() {
    let mut source = document();
    assert!(source
        .create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000905".into(),
            runtime_id: "ParamExtra".into(),
            name: "extra".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let imported = import_cdi3(&source, CDI).unwrap();
    let mut candidate = imported.candidate;
    assert!(candidate
        .set_parameter_display_name(PARAM_X, "新名".into())
        .status
        .is_ok());
    assert!(export_cdi3(&candidate).is_ok());
    let mut parameter = candidate.get_parameter(PARAM_X).unwrap().clone();
    parameter.runtime_id = "RenamedX".into();
    assert!(candidate.replace_parameter(parameter).status.is_ok());
    let error = export_cdi3(&candidate).unwrap_err();
    assert_eq!(error.code, "OPAQUE_NAMESPACE_CHANGED");
    assert_eq!(error.path, "$.Future");
    let restored = decode_project(&encode_project(&candidate).unwrap()).unwrap();
    assert_eq!(
        export_cdi3(&restored).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );

    let numeric = r#"{"Version":3,"Future":{"Gain":0.5}}"#;
    let imported = import_cdi3(&source, numeric).unwrap();
    assert!(imported
        .diagnostics
        .iter()
        .any(|d| d.code == "UnsupportedExtensionNumber" && d.path == "$.Future.Gain"));
    let reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    let error = export_cdi3(&reopened).unwrap_err();
    assert_eq!(error.code, "UnsupportedExtensionNumber");
    assert_eq!(error.path, "$.Future.Gain");

    let mut added = import_cdi3(&source, CDI).unwrap().candidate;
    assert!(added
        .create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000906".into(),
            runtime_id: "AddedOutsideCdi".into(),
            name: "new".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert_eq!(
        export_cdi3(&added).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
    let mut deleted = import_cdi3(&source, CDI).unwrap().candidate;
    assert!(deleted
        .erase_object("00000000-0000-4000-8000-000000000905")
        .status
        .is_ok());
    assert_eq!(
        export_cdi3(&deleted).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn runtime_id_not_display_name_is_export_identity_without_opaque_extensions() {
    let mut source = document();
    let mut part = source.get_part(PART).unwrap().clone();
    part.runtime_id = "ParamX".into();
    assert!(source.replace_part(part).status.is_ok());
    let cdi = r#"{
      "Version":3,
      "Parameters":[{"Id":"ParamX","GroupId":"","Name":"同名"},{"Id":"ParamY","GroupId":"","Name":"同名"}],
      "Parts":[{"Id":"ParamX","Name":"同名"}],
      "CombinedParameters":[["ParamX","ParamY"]]
    }"#;
    let mut candidate = import_cdi3(&source, cdi).unwrap().candidate;
    let mut param = candidate.get_parameter(PARAM_X).unwrap().clone();
    param.runtime_id = "RenamedX".into();
    assert!(candidate.replace_parameter(param).status.is_ok());
    let output: Value = serde_json::from_str(&export_cdi3(&candidate).unwrap()).unwrap();
    assert_eq!(output["Parameters"][0]["Id"], "RenamedX");
    assert_eq!(output["CombinedParameters"][0][0], "RenamedX");
    assert_eq!(output["Parts"][0]["Id"], "ParamX");
    assert_eq!(output["Parameters"][0]["Name"], "同名");
}

#[test]
fn optional_collections_unresolved_parts_and_duplicate_members_are_distinct() {
    let source = document();
    let absent = import_cdi3(&source, r#"{"Version":3}"#).unwrap();
    let absent_wire: Value =
        serde_json::from_str(&export_cdi3(&absent.candidate).unwrap()).unwrap();
    assert!(absent_wire.get("Parameters").is_none());
    assert!(absent_wire.get("Parts").is_none());
    let empty = import_cdi3(&source, r#"{"Version":3,"Parameters":[],"Parts":[]}"#).unwrap();
    let empty_wire: Value = serde_json::from_str(&export_cdi3(&empty.candidate).unwrap()).unwrap();
    assert_eq!(empty_wire["Parameters"], json!([]));
    assert_eq!(empty_wire["Parts"], json!([]));

    let unresolved = import_cdi3(
        &source,
        r#"{"Version":3,"Parts":[{"Id":"MissingPart","Name":"未找到"}]}"#,
    )
    .unwrap();
    assert_eq!(unresolved.diagnostics[0].code, "UNRESOLVED_PART");
    let error = export_cdi3(&unresolved.candidate).unwrap_err();
    assert_eq!(error.code, "UNRESOLVED_PART");
    assert_eq!(error.path, "$.Parts[0].Id");

    let repeated = import_cdi3(
        &source,
        r#"{"Version":3,"CombinedParameters":[["ParamX","ParamX"]]}"#,
    )
    .unwrap_err();
    assert_eq!(repeated.code, "DuplicateCombinationMember");
    assert_eq!(repeated.path, "$.CombinedParameters[0][1]");
}

#[test]
fn invalid_or_unpersistable_cdi_fails_without_changing_source() {
    let source = document();
    let before = source.checkpoint().estimated_bytes();
    let bad_group = r#"{"Version":3,"ParameterGroups":[{"Id":"A","GroupId":"B","Name":"a"}]}"#;
    assert_eq!(
        import_cdi3(&source, bad_group).unwrap_err().path,
        "$.ParameterGroups[0].GroupId"
    );
    let nul_name = r#"{"Version":3,"Parts":[{"Id":"PartA","Name":"bad\u0000"}]}"#;
    let error = import_cdi3(&source, nul_name).unwrap_err();
    assert_eq!(error.code, "CDI_NUL_UNPERSISTABLE");
    assert_eq!(error.path, "$.Parts[0].Name");
    let nul_extension = r#"{"Version":3,"Future":{"Deep":{"Name":"bad\u0000"}}}"#;
    assert_eq!(
        import_cdi3(&source, nul_extension).unwrap_err().path,
        "$.Future.Deep.Name"
    );
    assert_eq!(source.checkpoint().estimated_bytes(), before);
    assert_eq!(source.get_part(PART).unwrap().name, "Part A");

    let control = r#"{"Version":3,"Parts":[{"Id":"PartA","Name":"bad\u0001"}]}"#;
    let imported = import_cdi3(&source, control).unwrap();
    assert!(imported.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "UnsupportedControlCharacter" && diagnostic.path == "$.Parts[0].Name"
    }));
    let reopened = decode_project(&encode_project(&imported.candidate).unwrap()).unwrap();
    assert_eq!(reopened.get_part(PART).unwrap().name, "bad\u{1}");
    let error = export_cdi3(&reopened).unwrap_err();
    assert_eq!(error.code, "UnsupportedControlCharacter");
    assert_eq!(error.path, "$.Parts[0].Name");
}

#[test]
fn v6_is_strict_and_legacy_versions_cannot_smuggle_display_info() {
    let source = document();
    let mut wire: Value = serde_json::from_str(&encode_project(&source).unwrap()).unwrap();
    assert_eq!(wire["format_version"], 6);
    assert!(wire["document"]["display_info"].is_object());
    for (value, expected_code) in [
        (Value::Null, "UNSUPPORTED_VERSION"),
        (json!({}), "INVALID_PROJECT"),
        (json!({"origin":"generated"}), "INVALID_PROJECT"),
    ] {
        let mut old = wire.clone();
        old["format_version"] = json!(4);
        old["document"]
            .as_object_mut()
            .unwrap()
            .remove("animation_assets");
        old["document"]["display_info"] = value;
        assert_eq!(
            decode_project(&old.to_string()).unwrap_err().code,
            expected_code
        );
    }
    let mut old = wire.clone();
    old["format_version"] = json!(4);
    old["document"]
        .as_object_mut()
        .unwrap()
        .remove("display_info");
    old["document"]["animation_assets"] = Value::Null;
    assert_eq!(
        decode_project(&old.to_string()).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );
    let mut old = wire.clone();
    old["format_version"] = json!(4);
    old["document"]
        .as_object_mut()
        .unwrap()
        .remove("display_info");
    old["document"]
        .as_object_mut()
        .unwrap()
        .remove("animation_assets");
    let legacy = decode_project(&old.to_string()).unwrap();
    assert_eq!(legacy.display_info(), &DisplayInfo::default());
    assert_eq!(
        serde_json::from_str::<Value>(&encode_project(&legacy).unwrap()).unwrap()["format_version"],
        6
    );
    old["document"]["legacy_extra"] = json!({"still_accepted":true});
    assert!(decode_project(&old.to_string()).is_ok());

    let mut future_root = wire.clone();
    future_root["future_root"] = json!(true);
    assert_eq!(
        decode_project(&future_root.to_string()).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );

    wire["document"]["future_field"] = json!([]);
    assert_eq!(
        decode_project(&wire.to_string()).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );
    wire["document"]
        .as_object_mut()
        .unwrap()
        .remove("future_field");
    wire["document"]["display_info"]["future_field"] = json!(true);
    assert_eq!(
        decode_project(&wire.to_string()).unwrap_err().code,
        "INVALID_PROJECT"
    );
}

#[cfg(feature = "binary-prototype")]
#[test]
fn cbor_v4_cannot_smuggle_v5_container() {
    use kasane_project::{decode_project_cbor, encode_project_cbor};
    let source = document();
    let bytes = encode_project_cbor(&source).unwrap();
    assert!(decode_project_cbor(&bytes).unwrap().same_content(&source));
    let imported = import_cdi3(&source, CDI).unwrap().candidate;
    let bytes = encode_project_cbor(&imported).unwrap();
    let reopened = decode_project_cbor(&bytes).unwrap();
    assert!(reopened.same_content(&imported));
    assert_eq!(
        serde_json::from_str::<Value>(&export_cdi3(&reopened).unwrap()).unwrap(),
        serde_json::from_str::<Value>(CDI).unwrap()
    );
    let mut wire: Value = serde_json::from_str(&encode_project(&source).unwrap()).unwrap();
    wire["format_version"] = json!(4);
    wire["document"]["display_info"] = Value::Null;
    let mut payload = Vec::new();
    ciborium::into_writer(&wire, &mut payload).unwrap();
    let mut bad = b"KASCBOR1".to_vec();
    bad.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bad.extend_from_slice(&payload);
    assert_eq!(
        decode_project_cbor(&bad).unwrap_err().code,
        "UNSUPPORTED_VERSION"
    );
}
