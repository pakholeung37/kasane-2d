use kasane_core::{Canvas, PreviewValues, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, SourceSpace};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000001";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000002";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000003";
fn fixture_png() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sdk/asymmetric-2x2.png")
}

fn session() -> AuthoringSession {
    AuthoringSession::new(
        DOCUMENT_ID,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap()
}

#[test]
fn creates_png_rectangle_evaluates_and_undoes_all_content() {
    let path = fixture_png();
    let asset = prepare_png_asset(ASSET_ID, "asymmetric", &path).unwrap();
    assert_eq!((asset.width, asset.height), (2, 2));
    assert_eq!(asset.sha256.len(), 64);
    let mesh = rectangle_mesh(
        MESH_ID,
        "face",
        ASSET_ID,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let mut sdk = session();
    let before = sdk.version();
    let (_, receipt) = sdk
        .edit("create", Some(before), |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)?;
            Ok(())
        })
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert_eq!(sdk.asset_ids(), &[ASSET_ID]);
    assert_eq!(sdk.asset(ASSET_ID).unwrap().sha256.len(), 64);
    assert_eq!(sdk.mesh_ids(), &[MESH_ID]);
    assert_eq!(
        sdk.mesh(MESH_ID).unwrap().triangles,
        vec![[0, 1, 2], [0, 2, 3]]
    );
    let source = sdk.geometry(MESH_ID).unwrap();
    assert_eq!(source.space, SourceSpace::CanvasPixels);
    assert_eq!(source.vertex_ids, vec![0, 1, 2, 3]);
    let frame = sdk.evaluate(&PreviewValues::new()).unwrap();
    assert_eq!(frame.drawables.len(), 1);
    assert_eq!(frame.drawables[0].positions[0], Vec2::new(-1.0, 1.0));
    assert_eq!(frame.drawables[0].positions[2], Vec2::new(1.0, -1.0));

    let geometry = sdk.mesh(MESH_ID).unwrap();
    let moved: Vec<_> = geometry
        .base_positions
        .iter()
        .map(|p| Vec2::new(p.x + 10.0, p.y))
        .collect();
    sdk.edit("move", None, |edit| {
        edit.update_positions(MESH_ID, &geometry.vertex_ids, &moved)
    })
    .unwrap();
    assert_eq!(
        sdk.evaluate(&PreviewValues::new()).unwrap().drawables[0].positions[0],
        Vec2::new(0.0, 1.0)
    );
    assert_eq!(sdk.history_lengths(), (2, 0));
    sdk.undo().unwrap();
    assert_eq!(
        sdk.mesh(MESH_ID).unwrap().base_positions,
        geometry.base_positions
    );
    sdk.undo().unwrap();
    assert!(sdk.mesh_ids().is_empty());
    assert!(sdk.asset_ids().is_empty());
    sdk.redo().unwrap();
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH_ID).unwrap().base_positions, moved);
    assert_eq!(sdk.drain_events().len(), 6);
}

#[test]
fn failed_or_abandoned_edit_never_publishes_or_clears_redo() {
    let mut sdk = session();
    let before = sdk.version();
    let invalid = rectangle_mesh(
        MESH_ID,
        "orphan",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
    )
    .unwrap();
    {
        let mut edit = sdk.begin_edit("broken", Some(before)).unwrap();
        assert_eq!(
            edit.create_mesh(invalid).unwrap_err().code.as_ref(),
            "MISSING_ASSET"
        );
        assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    }
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert!(sdk.drain_events().is_empty());

    let path = fixture_png();
    let asset = prepare_png_asset(ASSET_ID, "fixture", &path).unwrap();
    {
        let mut edit = sdk.begin_edit("abandoned", None).unwrap();
        edit.create_asset(asset.clone()).unwrap();
    }
    assert!(sdk.asset_ids().is_empty());
    sdk.edit("asset", None, |edit| edit.create_asset(asset))
        .unwrap();
    sdk.undo().unwrap();
    let redo_version = sdk.version();
    let (_, noop) = sdk.edit("noop", None, |_edit| Ok(())).unwrap();
    assert!(!noop.changed);
    assert_eq!(sdk.version(), redo_version);
    assert_eq!(sdk.history_lengths(), (0, 1));
    sdk.redo().unwrap();
    assert_eq!(sdk.asset_ids(), &[ASSET_ID]);
}

#[test]
fn stale_version_and_invalid_position_are_rejected_without_partial_write() {
    let path = fixture_png();
    let asset = prepare_png_asset(ASSET_ID, "fixture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "face",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let mut sdk = session();
    let stale = sdk.version();
    sdk.edit("create", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })
    .unwrap();
    let error = sdk.begin_edit("stale", Some(stale)).err().unwrap();
    assert_eq!(error.code.as_ref(), "STALE_VERSION");
    assert_eq!(error.expected_version.as_deref(), Some(&stale));
    assert_eq!(error.actual_version.as_deref(), Some(&sdk.version()));
    let version = sdk.version();
    let original = sdk.mesh(MESH_ID).unwrap().base_positions;
    let mut edit = sdk.begin_edit("bad", None).unwrap();
    assert_eq!(
        edit.update_positions(MESH_ID, &[0], &[Vec2::new(f32::NAN, 0.0)])
            .unwrap_err()
            .code
            .as_ref(),
        "NON_FINITE"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.mesh(MESH_ID).unwrap().base_positions, original);
}
