use std::sync::Arc;

use kasane_core::{draw_order::DrawOrderGroup, Canvas, Vec2};
use kasane_sdk::{
    prepare_png_asset, rectangle_mesh, AuthoringSession, GeometryBounds, GeometryChecks,
    GeometryDiagnosticKind, ObjectKind,
};

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
    assert!(sdk.validate_structure().is_empty());
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

#[test]
fn identical_same_batch_recreation_is_an_identity_change() {
    let mut sdk = session();
    let old = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    let mesh = sdk.mesh(&id(3)).unwrap();
    let before = sdk.version();
    let (_, receipt) = sdk
        .edit("recreate", Some(before), |edit| {
            edit.erase_object(&id(3))?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert_eq!(sdk.mesh_ids(), &[id(3)]);
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let current = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    sdk.undo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&current).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    sdk.redo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&current).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}

#[test]
fn geometry_warnings_are_separate_from_structural_validity() {
    let mut sdk = session();
    let mut mesh = sdk.mesh(&id(3)).unwrap();
    mesh.triangles[1] = [0, 3, 2];
    sdk.edit("flip triangle", None, |edit| edit.replace_mesh(mesh))
        .unwrap();
    assert!(sdk.validate_structure().is_empty());
    let version = sdk.version();
    let warnings = sdk.diagnose_geometry(GeometryChecks::default()).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].kind,
        GeometryDiagnosticKind::InconsistentWinding
    );
    assert_eq!(warnings[0].triangle_index, Some(1));

    let warnings = sdk
        .diagnose_geometry(GeometryChecks {
            min_triangle_area: 201.0,
            canvas_bounds: Some(GeometryBounds {
                min: Vec2::new(0.0, 0.0),
                max: Vec2::new(50.0, 50.0),
            }),
        })
        .unwrap();
    assert_eq!(
        warnings
            .iter()
            .filter(|issue| issue.kind == GeometryDiagnosticKind::SmallTriangle)
            .count(),
        2
    );
    assert!(warnings
        .iter()
        .any(|issue| issue.kind == GeometryDiagnosticKind::OutsideCanvasBounds));
    assert_eq!(
        sdk.diagnose_geometry(GeometryChecks {
            min_triangle_area: -1.0,
            canvas_bounds: None,
        })
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_DIAGNOSTIC_OPTIONS"
    );
    assert_eq!(sdk.version(), version);
}

#[test]
fn new_project_failure_preserves_session_and_success_changes_generation() {
    let mut sdk = session();
    let old = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    let frame = sdk.preview_frame().unwrap();
    let current = sdk.version();
    let mut invalid_canvas = sdk.canvas();
    invalid_canvas.pixels_per_unit = 0.0;
    assert_eq!(
        sdk.new_project(&id(1), invalid_canvas, Some(current))
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_CANVAS"
    );
    let mut stale = current;
    stale.revision += 1;
    assert_eq!(
        sdk.new_project(&id(1), sdk.canvas(), Some(stale))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_VERSION"
    );
    assert_eq!(sdk.version(), current);
    sdk.resolve_handle(&old).unwrap();
    assert!(Arc::ptr_eq(&frame, &sdk.preview_frame().unwrap()));
    assert_eq!(sdk.history_lengths(), (1, 0));

    let next = sdk
        .new_project(&id(1), sdk.canvas(), Some(current))
        .unwrap();
    assert_eq!(next.session_id, current.session_id);
    assert_eq!(next.generation, current.generation + 1);
    assert!(sdk.asset_ids().is_empty());
    assert!(sdk.mesh_ids().is_empty());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert!(sdk.preview_values().is_empty());
    assert!(sdk.drain_events().is_empty());
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}
