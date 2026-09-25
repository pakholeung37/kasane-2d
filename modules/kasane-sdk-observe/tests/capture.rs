use kasane_core::{Canvas, Parameter, PreviewValues, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession};
use kasane_sdk_observe::{
    CanvasRoi, HistoryStatus, ObservationInput, ObservationSource, Observer, ObserverConfig,
    RenderRequest, ResolvedObservation,
};

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";
const PARAMETER: &str = "00000000-0000-4000-8000-000000000004";

#[test]
fn batch_samples_one_document_snapshot_and_reuses_capture_identity() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let png =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET, "texture", &png).unwrap();
    let mesh = rectangle_mesh(
        MESH,
        "face",
        ASSET,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    session
        .edit("fixture", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)?;
            edit.create_parameter(Parameter {
                id: PARAMETER.into(),
                name: "Shift".into(),
                ..Parameter::default()
            })
        })
        .unwrap();
    let snapshot = session.read_snapshot();
    let requested = [
        ObservationInput::resolve_requested(
            &snapshot,
            &PreviewValues::from([("Shift".into(), 0.0)]),
        )
        .unwrap(),
        ObservationInput::resolve_requested(
            &snapshot,
            &PreviewValues::from([("Shift".into(), 0.5)]),
        )
        .unwrap(),
    ];
    session
        .edit("change after snapshot", None, |edit| {
            edit.update_positions(MESH, &[0], &[Vec2::new(50.0, 40.0)])
        })
        .unwrap();
    let inputs = ObservationInput::capture_samples(&snapshot, &requested).unwrap();
    assert_eq!(inputs[0].version(), inputs[1].version());
    assert_ne!(inputs[0].version(), session.version());
    assert_eq!(
        inputs[0].frame().drawables[0].positions[0],
        Vec2::new(-1.0, 1.0)
    );
    assert_eq!(inputs[1].frame().parameters[0].value, 0.5);
    let scenes = ResolvedObservation::capture_many(inputs).unwrap();
    assert_eq!(scenes[0].capture_id(), scenes[1].capture_id());
    assert_ne!(scenes[0].scene_digest(), scenes[1].scene_digest());
    assert_eq!(
        scenes[0].textures()[0].data.sha256,
        scenes[1].textures()[0].data.sha256
    );
    let fresh = ObservationInput::capture(&session, &PreviewValues::new()).unwrap();
    assert_eq!(
        ResolvedObservation::capture_many(vec![scenes[0].input().clone(), fresh])
            .unwrap_err()
            .code,
        "MIXED_DOCUMENT_SNAPSHOTS"
    );
}

#[test]
fn captures_unpublished_project_and_keeps_old_frame_after_edit() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let png =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET, "texture", &png).unwrap();
    let mesh = rectangle_mesh(
        MESH,
        "face",
        ASSET,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    session
        .edit("create", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    let input = ObservationInput::capture(&session, &PreviewValues::new()).unwrap();
    assert_eq!(
        input.frame().drawables[0].positions[0],
        Vec2::new(-1.0, 1.0)
    );
    assert_eq!(input.assets().len(), 1);
    let textures = input.resolve_textures().unwrap();
    assert_eq!((textures[0].data.width, textures[0].data.height), (2, 2));
    assert_eq!(textures[0].data.sha256, input.assets()[0].sha256);
    session
        .edit("move", None, |edit| {
            edit.update_positions(MESH, &[0], &[Vec2::new(50.0, 40.0)])
        })
        .unwrap();
    assert_ne!(input.version(), session.version());
    assert_eq!(
        input.frame().drawables[0].positions[0],
        Vec2::new(-1.0, 1.0)
    );
    assert_eq!(
        session.evaluate(&PreviewValues::new()).unwrap().drawables[0].positions[0],
        Vec2::new(0.0, 1.0)
    );
}

#[test]
fn captures_animation_frame_and_rejects_stale_preview() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let png =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET, "texture", &png).unwrap();
    let mesh = rectangle_mesh(
        MESH,
        "face",
        ASSET,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    session
        .edit("create", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    let mut preview = session.motion_preview();
    let input = ObservationInput::capture_motion(&session, &preview).unwrap();
    assert_eq!(input.frame(), &preview.evaluate_drawables().unwrap());
    assert!(matches!(
        input.source(),
        ObservationSource::Animation {
            apply_model_opacity: false,
            history_status,
            snapshot,
            operation: Some(operation),
        } if *history_status == HistoryStatus::NotRecorded
            && snapshot.as_ref() == preview.snapshot()
            && operation == preview.operation()
    ));
    let scene = ResolvedObservation::capture(input.clone()).unwrap();
    let second_preview = session.motion_preview();
    let second = ResolvedObservation::capture(
        ObservationInput::capture_motion(&session, &second_preview).unwrap(),
    )
    .unwrap();
    assert_ne!(
        preview.operation().preview_id,
        second_preview.operation().preview_id
    );
    assert_ne!(scene.capture_id(), second.capture_id());
    assert_eq!(scene.scene_digest(), second.scene_digest());

    preview.reset();
    let reset =
        ResolvedObservation::capture(ObservationInput::capture_motion(&session, &preview).unwrap())
            .unwrap();
    assert_ne!(input.source(), reset.input().source());
    assert_eq!(scene.scene_digest(), reset.scene_digest());
    let bundle_directory = std::env::temp_dir().join(format!(
        "kasane-observe-animation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    scene.save_scene(&bundle_directory).unwrap();
    let reopened = ResolvedObservation::open_scene(&bundle_directory).unwrap();
    assert_eq!(reopened.input().source(), input.source());
    std::fs::remove_dir_all(bundle_directory).unwrap();
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
        fit_long_side: 64.0,
    })
    .unwrap();
    let animated = observer.observe(&input).unwrap();
    let static_frame = observer
        .observe(&ObservationInput::capture(&session, &PreviewValues::new()).unwrap())
        .unwrap();
    assert_eq!(animated.rgba, static_frame.rgba);

    session
        .edit("move", None, |edit| {
            edit.update_positions(MESH, &[0], &[Vec2::new(50.0, 40.0)])
        })
        .unwrap();
    assert_eq!(
        ObservationInput::capture_motion(&session, &preview)
            .unwrap_err()
            .code,
        "STALE_ANIMATION_PREVIEW"
    );
}

#[test]
fn renders_unsaved_session_and_reuses_gpu_across_changes() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let png =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET, "texture", &png).unwrap();
    let mesh = rectangle_mesh(
        MESH,
        "face",
        ASSET,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    session
        .edit("create", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
        fit_long_side: 64.0,
    })
    .unwrap();
    let original = ObservationInput::capture(&session, &PreviewValues::new()).unwrap();
    let first = observer.observe(&original).unwrap();
    assert_eq!(first.rgba.len(), 64 * 64 * 4);
    assert!(first
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] > 0));
    assert_eq!(first.version, original.version());
    let initial_texture_revision = first.texture_revisions[0].revision;

    session
        .edit("move", None, |edit| {
            edit.update_positions(
                MESH,
                &[0, 1, 2, 3],
                &[
                    Vec2::new(50.0, 40.0),
                    Vec2::new(70.0, 40.0),
                    Vec2::new(70.0, 60.0),
                    Vec2::new(50.0, 60.0),
                ],
            )
        })
        .unwrap();
    let moved = ObservationInput::capture(&session, &PreviewValues::new()).unwrap();
    let second = observer.observe(&moved).unwrap();
    assert_ne!(first.rgba, second.rgba);
    assert_eq!(
        second.texture_revisions[0].revision,
        initial_texture_revision
    );
    assert_ne!(second.version, first.version);

    let new_png =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/texture_00.png");
    let replacement = prepare_png_asset(ASSET, "texture", &new_png).unwrap();
    session
        .edit("texture", None, |edit| edit.replace_asset(replacement))
        .unwrap();
    let changed_texture = ObservationInput::capture(&session, &PreviewValues::new()).unwrap();
    let third = observer.observe(&changed_texture).unwrap();
    assert!(third.texture_revisions[0].revision > initial_texture_revision);
    assert_ne!(
        third.texture_revisions[0].sha256,
        first.texture_revisions[0].sha256
    );
    observer.set_fit_long_side(32.0).unwrap();
    let fourth = observer.observe(&changed_texture).unwrap();
    assert_ne!(third.rgba, fourth.rgba);
    assert_eq!(third.texture_revisions, fourth.texture_revisions);
}

#[test]
fn resolved_capture_rerenders_roi_after_source_disappears_and_restores_legacy_view() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let temporary = std::env::temp_dir().join(format!(
        "kasane-observe-roi-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::copy(&source, &temporary).unwrap();
    let asset = prepare_png_asset(ASSET, "texture", &temporary).unwrap();
    let mesh = rectangle_mesh(
        MESH,
        "face",
        ASSET,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    session
        .edit("create", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    let input = ObservationInput::capture(&session, &PreviewValues::new()).unwrap();
    let captured = ResolvedObservation::capture(input).unwrap();
    let duplicate = ResolvedObservation::capture(captured.input().clone()).unwrap();
    assert_ne!(captured.capture_id(), duplicate.capture_id());
    assert_eq!(captured.scene_digest(), duplicate.scene_digest());
    let scene_directory = temporary.with_extension("scene");
    captured.save_scene(&scene_directory).unwrap();
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
        fit_long_side: 64.0,
    })
    .unwrap();
    let legacy_before = observer.observe(captured.input()).unwrap();
    std::fs::remove_file(&temporary).unwrap();
    let request = RenderRequest {
        width: 128,
        height: 96,
        roi: CanvasRoi {
            x0: 38.0,
            y0: 38.0,
            x1: 62.0,
            y1: 62.0,
        },
        padding_canvas: 2.0,
    };
    let enlarged = observer.render(&captured, request).unwrap();
    assert_eq!((enlarged.width, enlarged.height), (128, 96));
    assert_eq!(enlarged.explicit_view.unwrap().requested_roi, request.roi);
    assert!(enlarged.view_scale > 64.0 / 100.0);
    assert!(enlarged.drawable_bounds[0].bounds.is_some());
    let repeated = observer.render(&captured, request).unwrap();
    assert_eq!(repeated.rgba, enlarged.rgba);
    let reopened = ResolvedObservation::open_scene(&scene_directory).unwrap();
    assert_eq!(reopened.capture_id(), captured.capture_id());
    assert_eq!(reopened.scene_digest(), captured.scene_digest());
    assert_eq!(
        reopened.render_digest(request).unwrap(),
        captured.render_digest(request).unwrap()
    );
    assert_ne!(
        captured.render_digest(request).unwrap(),
        captured
            .render_digest(RenderRequest {
                width: 129,
                ..request
            })
            .unwrap()
    );
    assert_eq!(
        observer.render(&reopened, request).unwrap().rgba,
        enlarged.rgba
    );
    assert_eq!(reopened.input().authoring(), captured.input().authoring());
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "bundle_child_process", "--nocapture"])
        .env("KASANE_OBSERVE_TEST_SCENE", &scene_directory)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
    assert!(String::from_utf8_lossy(&child.stdout).contains("BUNDLE_REOPEN_OK"));
    assert_eq!(
        observer.observe(captured.input()).unwrap_err().code,
        "PROJECT_IO"
    );
    std::fs::copy(source, &temporary).unwrap();
    let legacy_after = observer.observe(captured.input()).unwrap();
    assert_eq!(legacy_after.rgba, legacy_before.rgba);
    assert_eq!(legacy_after.input_sha256, legacy_before.input_sha256);
    assert!(legacy_after.explicit_view.is_none());
    std::fs::remove_file(temporary).unwrap();
    let manifest_path = scene_directory.join("scene.json");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    let mut previous: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    previous["schema_version"] = serde_json::json!(1);
    previous.as_object_mut().unwrap().remove("capture_id");
    previous.as_object_mut().unwrap().remove("scene_digest");
    std::fs::write(&manifest_path, serde_json::to_vec(&previous).unwrap()).unwrap();
    let reopened_v1 = ResolvedObservation::open_scene(&scene_directory).unwrap();
    assert_eq!(reopened_v1.scene_digest(), captured.scene_digest());
    assert_ne!(reopened_v1.capture_id(), captured.capture_id());
    std::fs::write(&manifest_path, &manifest).unwrap();
    std::fs::write(
        &manifest_path,
        manifest.replace("\"schema_version\": 2", "\"schema_version\": 99"),
    )
    .unwrap();
    assert_eq!(
        ResolvedObservation::open_scene(&scene_directory)
            .unwrap_err()
            .code,
        "UNSUPPORTED_BUNDLE_VERSION"
    );
    std::fs::write(
        &manifest_path,
        manifest.replace("texture-000.png", "../outside.png"),
    )
    .unwrap();
    assert_eq!(
        ResolvedObservation::open_scene(&scene_directory)
            .unwrap_err()
            .code,
        "BUNDLE_FORMAT"
    );
    let mut changed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    changed["document_id"] = serde_json::json!("changed-document");
    std::fs::write(&manifest_path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(
        ResolvedObservation::open_scene(&scene_directory)
            .unwrap_err()
            .code,
        "BUNDLE_HASH_MISMATCH"
    );
    let mut oversized: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    oversized["textures"][0]["asset"]["width"] = serde_json::json!(u32::MAX);
    oversized["textures"][0]["asset"]["height"] = serde_json::json!(u32::MAX);
    std::fs::write(&manifest_path, serde_json::to_vec(&oversized).unwrap()).unwrap();
    assert_eq!(
        ResolvedObservation::open_scene(&scene_directory)
            .unwrap_err()
            .code,
        "OBSERVATION_BUDGET_EXCEEDED"
    );
    std::fs::write(&manifest_path, manifest).unwrap();
    let texture_path = scene_directory.join("texture-000.png");
    std::fs::write(&texture_path, b"changed").unwrap();
    assert!(matches!(
        ResolvedObservation::open_scene(&scene_directory)
            .unwrap_err()
            .code
            .as_str(),
        "BUNDLE_FORMAT" | "BUNDLE_HASH_MISMATCH"
    ));
    std::fs::remove_dir_all(scene_directory).unwrap();
}

#[test]
fn bundle_child_process() {
    let Ok(path) = std::env::var("KASANE_OBSERVE_TEST_SCENE") else {
        return;
    };
    let captured = ResolvedObservation::open_scene(std::path::Path::new(&path)).unwrap();
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
        fit_long_side: 64.0,
    })
    .unwrap();
    let frame = observer
        .render(
            &captured,
            RenderRequest {
                width: 128,
                height: 96,
                roi: CanvasRoi {
                    x0: 38.0,
                    y0: 38.0,
                    x1: 62.0,
                    y1: 62.0,
                },
                padding_canvas: 2.0,
            },
        )
        .unwrap();
    assert!(frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] != 0));
    println!("BUNDLE_REOPEN_OK");
}

#[test]
fn unhashed_source_is_hashed_in_bundle_and_reopens() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let mut asset = prepare_png_asset(ASSET, "texture", &source).unwrap();
    asset.sha256.clear();
    session
        .edit("unhashed texture", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(
                rectangle_mesh(
                    MESH,
                    "face",
                    ASSET,
                    Vec2::new(40.0, 40.0),
                    Vec2::new(60.0, 60.0),
                )
                .unwrap(),
            )
        })
        .unwrap();
    let capture = ResolvedObservation::capture(
        ObservationInput::capture(&session, &PreviewValues::new()).unwrap(),
    )
    .unwrap();
    let directory = std::env::temp_dir().join(format!("observe-unhashed-{}", uuid::Uuid::new_v4()));
    capture.save_scene(&directory).unwrap();
    let reopened = ResolvedObservation::open_scene(&directory).unwrap();
    assert_eq!(capture.scene_digest(), reopened.scene_digest());
    assert_eq!(
        reopened.textures()[0].asset.sha256,
        capture.textures()[0].data.sha256
    );
    std::fs::remove_dir_all(directory).unwrap();
}
