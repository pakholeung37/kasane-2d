use kasane_project::DocumentSession;
use serde_json::Value;

#[test]
fn model3_groups_layout_and_hit_areas_survive_project_and_package() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/external_v50");
    let temporary =
        std::env::temp_dir().join(format!("kasane-model3-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    std::fs::copy(source.join("model.moc3"), temporary.join("model.moc3")).unwrap();
    std::fs::copy(
        source.join("texture_00.png"),
        temporary.join("texture_00.png"),
    )
    .unwrap();
    let mut model3: Value =
        serde_json::from_slice(&std::fs::read(source.join("model.model3.json")).unwrap()).unwrap();
    model3["FileReferences"]
        .as_object_mut()
        .unwrap()
        .remove("Physics");
    model3["FileReferences"]
        .as_object_mut()
        .unwrap()
        .remove("Motions");
    model3["Groups"] =
        serde_json::json!([{"Target":"Parameter","Name":"EyeBlink","Ids":["ParamX"]}]);
    model3["Layout"] = serde_json::json!({"CenterX":0.0,"Width":2.0});
    model3["HitAreas"] = serde_json::json!([{"Id":"Mesh0","Name":"Head"}]);
    std::fs::write(
        temporary.join("model.model3.json"),
        serde_json::to_vec_pretty(&model3).unwrap(),
    )
    .unwrap();
    let mut session = DocumentSession::new();
    let (result, _) = session.import_model3(&temporary.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(
        session.document().missing_attachments().is_empty(),
        "{:?}",
        session.document().missing_attachments()
    );
    let mesh_runtime = session
        .document()
        .get_mesh(&session.document().mesh_order()[0])
        .unwrap()
        .runtime_id
        .clone();
    let mut settings = session.document().model3_settings().clone();
    settings.hit_areas = Some(vec![kasane_core::document::ModelHitArea {
        name: "Head".into(),
        mesh: kasane_core::document::ModelTargetRef::Resolved {
            object_id: session.document().mesh_order()[0].clone(),
        },
        extensions: Default::default(),
    }]);
    assert!(session
        .document_mut()
        .set_model3_settings(settings)
        .status
        .is_ok());
    let project = temporary.join("project");
    assert!(session.save(&project).status.is_ok());
    let mut reopened = DocumentSession::new();
    assert!(reopened.open(&project).status.is_ok());
    let package = temporary.join("package");
    let result = reopened.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    let output: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(output["Groups"], model3["Groups"]);
    assert_eq!(output["Layout"], model3["Layout"]);
    assert_eq!(
        output["HitAreas"],
        serde_json::json!([{"Id": mesh_runtime, "Name": "Head"}])
    );
    let mut invalid = output;
    invalid["FileReferences"]["Physics"] = "../outside.physics3.json".into();
    std::fs::write(
        package.join("model.model3.json"),
        serde_json::to_vec(&invalid).unwrap(),
    )
    .unwrap();
    let mut rejected = DocumentSession::new();
    let (result, _) = rejected.import_model3(&package.join("model.model3.json"));
    assert_eq!(result.status.code, "INVALID_ATTACHMENT_PATH");
    std::fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn sound_and_userdata_are_managed_after_source_is_removed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/external_v50");
    let temporary =
        std::env::temp_dir().join(format!("kasane-managed-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(temporary.join("motions/audio")).unwrap();
    std::fs::copy(source.join("model.moc3"), temporary.join("model.moc3")).unwrap();
    std::fs::copy(
        source.join("texture_00.png"),
        temporary.join("texture_00.png"),
    )
    .unwrap();
    std::fs::copy(
        root.join("tests/fixtures/animation_cpu/loop.motion3.json"),
        temporary.join("idle.motion3.json"),
    )
    .unwrap();
    let sound = b"RIFF\0\0\0\0WAVE";
    let user_data = br#"{"Version":3,"UserData":[]}"#;
    std::fs::write(temporary.join("motions/audio/a.wav"), sound).unwrap();
    std::fs::write(temporary.join("data.userdata3.json"), user_data).unwrap();
    let mut model3: Value =
        serde_json::from_slice(&std::fs::read(source.join("model.model3.json")).unwrap()).unwrap();
    model3["FileReferences"]
        .as_object_mut()
        .unwrap()
        .remove("Physics");
    model3["FileReferences"]["Motions"] =
        serde_json::json!({"Idle":[{"File":"idle.motion3.json","Sound":"motions/audio/a.wav"}]});
    model3["FileReferences"]["UserData"] = "data.userdata3.json".into();
    std::fs::write(
        temporary.join("model.model3.json"),
        serde_json::to_vec_pretty(&model3).unwrap(),
    )
    .unwrap();
    let mut session = DocumentSession::new();
    let (result, _) = session.import_model3(&temporary.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(
        session.document().missing_attachments().is_empty(),
        "{:?}",
        session.document().missing_attachments()
    );
    assert_eq!(session.document().package_attachments().len(), 2);
    let project = temporary.join("project");
    assert!(session.save(&project).status.is_ok());
    for path in [
        "model.moc3",
        "texture_00.png",
        "idle.motion3.json",
        "motions/audio/a.wav",
        "data.userdata3.json",
    ] {
        std::fs::remove_file(temporary.join(path)).unwrap();
    }
    let mut reopened = DocumentSession::new();
    assert!(reopened.open(&project).status.is_ok());
    let package = temporary.join("package");
    let result = reopened.export_package(&package);
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(
        std::fs::read(package.join("motions/audio/a.wav")).unwrap(),
        sound
    );
    assert_eq!(
        std::fs::read(package.join("data.userdata3.json")).unwrap(),
        user_data
    );
    let mut imported = DocumentSession::new();
    let (result, _) = imported.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert!(imported.document().missing_attachments().is_empty());
    std::fs::remove_dir_all(temporary).unwrap();
}
