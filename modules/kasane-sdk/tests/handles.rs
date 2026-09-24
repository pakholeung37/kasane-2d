use kasane_core::{Canvas, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, ObjectKind};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000501";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000502";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000503";

fn session() -> AuthoringSession {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET_ID, "fixture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "mesh",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let mut sdk = AuthoringSession::new(
        DOCUMENT_ID,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    sdk.edit("create", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })
    .unwrap();
    sdk
}

#[test]
fn handle_survives_fields_but_never_revives_after_undo_redo() {
    let mut sdk = session();
    let mesh = sdk.handle(ObjectKind::Mesh, MESH_ID).unwrap();
    let asset = sdk.handle(ObjectKind::Asset, ASSET_ID).unwrap();
    assert_eq!(sdk.mesh_by_handle(&mesh).unwrap().name, "mesh");
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH_ID, "renamed"))
        .unwrap();
    assert_eq!(sdk.mesh_by_handle(&mesh).unwrap().name, "renamed");
    sdk.undo().unwrap();
    assert_eq!(sdk.mesh_by_handle(&mesh).unwrap().name, "mesh");
    sdk.undo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&mesh).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    assert_eq!(
        sdk.resolve_handle(&asset).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    sdk.redo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&mesh).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let current = sdk.handle(ObjectKind::Mesh, MESH_ID).unwrap();
    assert_ne!(mesh, current);
    assert_eq!(sdk.mesh_by_handle(&current).unwrap().name, "mesh");
}

#[test]
fn handle_is_bound_to_its_session_and_kind() {
    let first = session();
    let second = session();
    let handle = first.handle(ObjectKind::Mesh, MESH_ID).unwrap();
    assert_eq!(
        second.resolve_handle(&handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let asset = first.handle(ObjectKind::Asset, ASSET_ID).unwrap();
    assert_eq!(
        first.mesh_by_handle(&asset).unwrap_err().code.as_ref(),
        "WRONG_OBJECT_KIND"
    );
    assert_eq!(
        first
            .handle(ObjectKind::Parameter, MESH_ID)
            .unwrap_err()
            .code
            .as_ref(),
        "NOT_FOUND"
    );
}
