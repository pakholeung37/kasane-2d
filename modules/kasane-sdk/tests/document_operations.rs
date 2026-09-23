use kasane_core::{draw_order::DrawOrderGroup, Canvas, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, ObjectKind};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        &id(1),
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sdk/asymmetric-2x2.png");
    let asset = prepare_png_asset(&id(2), "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        &id(3),
        "mesh",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    sdk.edit("fixture", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })
    .unwrap();
    sdk
}

#[test]
fn canvas_and_draw_order_publish_together_and_reject_invalid_groups() {
    let mut sdk = session();
    let mut canvas = sdk.canvas();
    canvas.pixels_per_unit = 20.0;
    let groups = vec![DrawOrderGroup {
        owner: String::new(),
        items: vec![id(3)],
        min_order: -100,
        max_order: 100,
    }];
    let before = sdk.version();
    sdk.edit("drawing", Some(before), |edit| {
        edit.replace_canvas(canvas)?;
        edit.replace_draw_order_groups(groups.clone())
    })
    .unwrap();
    assert_eq!(sdk.version().revision, before.revision + 1);
    assert_eq!(sdk.canvas().pixels_per_unit, 20.0);
    assert_eq!(sdk.draw_order_groups(), Some(groups));
    assert_eq!(
        sdk.evaluate(&Default::default()).unwrap().drawables[0].positions[0].x,
        -0.5
    );

    let version = sdk.version();
    let mut edit = sdk.begin_edit("bad groups", None).unwrap();
    assert_eq!(
        edit.replace_draw_order_groups(vec![DrawOrderGroup {
            owner: String::new(),
            items: vec![],
            min_order: 0,
            max_order: 0,
        }])
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_DRAW_GROUP"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);
    sdk.undo().unwrap();
    assert_eq!(sdk.canvas().pixels_per_unit, 10.0);
    assert!(sdk.draw_order_groups().is_none());
}

#[test]
fn referenced_delete_is_rejected_and_recreated_id_does_not_revive_handle() {
    let mut sdk = session();
    let mesh = sdk.mesh(&id(3)).unwrap();
    let old_handle = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    assert_eq!(sdk.references_to(&id(2)), vec![id(3)]);
    let version = sdk.version();
    let mut edit = sdk.begin_edit("bad erase", None).unwrap();
    let error = edit.erase_object(&id(2)).unwrap_err();
    assert_eq!(error.code.as_ref(), "OBJECT_REFERENCED");
    assert_eq!(&*error.referrers, &[id(3)]);
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);

    sdk.edit("erase", None, |edit| edit.erase_object(&id(3)))
        .unwrap();
    assert!(sdk.mesh(&id(3)).is_none());
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    sdk.edit("recreate", None, |edit| edit.create_mesh(mesh))
        .unwrap();
    assert_eq!(sdk.mesh_ids(), &[id(3)]);
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let current = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    assert_ne!(current, old_handle);
    sdk.undo().unwrap();
    assert!(sdk.mesh(&id(3)).is_none());
    sdk.undo().unwrap();
    assert!(sdk.mesh(&id(3)).is_some());
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}
