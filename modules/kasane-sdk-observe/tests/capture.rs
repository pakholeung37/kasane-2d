use kasane_core::{Canvas, PreviewValues, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession};
use kasane_sdk_observe::{
    CanvasRoi, HistoryStatus, ObservationInput, ObservationSource, Observer, ObserverConfig,
    RenderRequest, ResolvedObservation,
};

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";

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
    let preview = session.motion_preview();
    let input = ObservationInput::capture_motion(&session, &preview).unwrap();
    assert_eq!(input.frame(), &preview.evaluate_drawables().unwrap());
    assert!(matches!(
        input.source(),
        ObservationSource::Animation {
            apply_model_opacity: false,
            history_status,
            snapshot,
        } if *history_status == HistoryStatus::NotRecorded && snapshot.as_ref() == preview.snapshot()
    ));
    let scene = ResolvedObservation::capture(input.clone()).unwrap();
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
    std::fs::write(
        &manifest_path,
        manifest.replace("\"schema_version\": 1", "\"schema_version\": 99"),
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
