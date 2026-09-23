use kasane_core::{Canvas, PreviewValues, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession};
use kasane_sdk_observe::{ObservationInput, Observer, ObserverConfig};

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
    let png = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sdk/asymmetric-2x2.png");
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
fn renders_unsaved_session_and_reuses_gpu_across_changes() {
    let mut session = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let png = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sdk/asymmetric-2x2.png");
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

    let new_png = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/external_v50/texture_00.png");
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
