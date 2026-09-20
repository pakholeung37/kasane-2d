use kasane_core::{Canvas, ChangeKind, Document, ImageAsset, Mesh, Vec2, VertexPositionUpdate};

const DOC: &str = "11111111-1111-4111-8111-111111111111";
const ASSET: &str = "22222222-2222-4222-8222-222222222222";
const MESH: &str = "33333333-3333-4333-8333-333333333333";

fn sample() -> Mesh {
    Mesh {
        id: MESH.to_string(),
        name: "quad".to_string(),
        texture_asset_id: ASSET.to_string(),
        vertex_ids: vec![40, 10, 90, 20],
        base_positions: vec![
            Vec2::new(-10.0, 10.0),
            Vec2::new(-10.0, -10.0),
            Vec2::new(10.0, -10.0),
            Vec2::new(10.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        ],
        triangles: vec![[40, 10, 90], [40, 90, 20]],
        ..Default::default()
    }
}

#[test]
fn test_core_lifecycle_and_topology() {
    let mut doc = Document::new();
    assert!(!doc.create_mesh(sample()).status.is_ok());
    assert!(!doc
        .initialize(
            "not-a-uuid",
            Canvas::new(100.0, 100.0, Vec2::default(), 1.0)
        )
        .is_ok());
    assert!(!doc
        .initialize(DOC, Canvas::new(0.0, 100.0, Vec2::default(), 1.0))
        .is_ok());
    assert!(doc
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 1.0))
        .is_ok());
    assert!(!doc
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 1.0))
        .is_ok());

    assert!(!doc.create_mesh(sample()).status.is_ok());
    assert!(!doc
        .add_asset(ImageAsset {
            id: DOC.to_string(),
            name: "bad".to_string(),
            source: "memory://test".to_string(),
            width: 32,
            height: 32,
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: ASSET.to_string(),
            name: "checker".to_string(),
            source: "memory://test".to_string(),
            width: 32,
            height: 32,
            ..Default::default()
        })
        .status
        .is_ok());

    let mut invalid = sample();
    invalid.vertex_ids[1] = 40;
    assert_eq!(doc.create_mesh(invalid).status.code, "DUPLICATE_VERTEX");

    let mut invalid = sample();
    invalid.triangles[0][0] = 99;
    assert_eq!(doc.create_mesh(invalid).status.code, "MISSING_VERTEX");

    let mut invalid = sample();
    invalid.triangles[0][0] = 10;
    assert_eq!(doc.create_mesh(invalid).status.code, "REPEATED_VERTEX");

    let mut invalid = sample();
    invalid.base_positions[0].x = f32::INFINITY;
    assert_eq!(doc.create_mesh(invalid).status.code, "NON_FINITE");

    assert!(doc.mesh_order().is_empty());

    let mut source = sample();
    let edit = doc.create_mesh(source.clone());
    assert!(edit.status.is_ok() && edit.changes.kind == ChangeKind::Structure);

    source.base_positions[0].x = 999.0;
    assert_eq!(doc.get_mesh(MESH).unwrap().base_positions[0].x, -10.0);
    assert!(!doc.create_mesh(sample()).status.is_ok());

    let indices = doc.render_indices(MESH).unwrap();
    assert_eq!(indices, vec![0, 1, 2, 0, 2, 3]);

    let revision = doc.revision();
    let before = doc.get_mesh(MESH).unwrap().clone();

    let ids = vec![40, 999];
    let values = vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)];
    assert_eq!(
        doc.set_vertex_positions(MESH, &ids, &values).status.code,
        "MISSING_VERTEX"
    );
    assert_eq!(
        doc.get_mesh(MESH).unwrap().base_positions,
        before.base_positions
    );
    assert_eq!(doc.revision(), revision);

    let ids = vec![40, 40];
    assert_eq!(
        doc.set_vertex_positions(MESH, &ids, &values).status.code,
        "DUPLICATE_VERTEX"
    );

    let ids = vec![40];
    assert_eq!(
        doc.set_vertex_positions(MESH, &ids, &values).status.code,
        "INVALID_LENGTH"
    );

    let values = vec![Vec2::new(f32::NAN, 0.0)];
    assert_eq!(
        doc.set_vertex_positions(MESH, &ids, &values).status.code,
        "NON_FINITE"
    );

    let values = vec![Vec2::new(-15.0, 12.0)];
    let edit = doc.set_vertex_positions(MESH, &ids, &values);
    assert!(edit.status.is_ok() && edit.changes.kind == ChangeKind::Positions);
    assert_eq!(doc.get_mesh(MESH).unwrap().base_positions[0], values[0]);
    assert_eq!(
        doc.get_mesh(MESH).unwrap().base_positions[1],
        before.base_positions[1]
    );

    let revision = doc.revision();
    assert_eq!(
        doc.set_vertex_positions(MESH, &ids, &values).changes.kind,
        ChangeKind::None
    );
    assert_eq!(doc.revision(), revision);

    assert_eq!(
        doc.rename_mesh(MESH, "renamed".to_string()).changes.kind,
        ChangeKind::Metadata
    );
    assert_eq!(doc.get_mesh(MESH).unwrap().id, MESH);
    assert_eq!(doc.get_mesh(MESH).unwrap().vertex_ids, before.vertex_ids);

    let mut collapsed = sample();
    collapsed.id = "44444444-4444-4444-8444-444444444444".to_string();
    collapsed.base_positions = vec![Vec2::default(); 4];
    collapsed.uvs[0] = Vec2::new(-1.0, 2.0);
    assert!(doc.create_mesh(collapsed.clone()).status.is_ok());
    assert_eq!(doc.mesh_order(), &[MESH.to_string(), collapsed.id.clone()]);
    assert!(doc.render_indices("missing").is_err());
    assert_eq!(doc.get_mesh(MESH).unwrap().base_positions[0], values[0]);

    // Clean state follows content, including edits that return to the saved value.
    doc.mark_saved();
    let saved_name = doc.get_mesh(MESH).unwrap().name.clone();
    assert!(
        doc.rename_mesh(MESH, "temporary".to_string())
            .status
            .is_ok()
            && doc.modified()
    );
    assert!(doc.rename_mesh(MESH, saved_name).status.is_ok() && !doc.modified());

    let saved_source = doc.clone();
    assert!(doc
        .rename_mesh(MESH, "second save".to_string())
        .status
        .is_ok());
    doc.mark_saved();
    let second_save = doc.clone();
    doc.restore_from(&saved_source);
    assert!(doc.modified());
    doc.restore_from(&second_save);
    assert!(!doc.modified());

    // Multiple commands commit atomically and occupy one history step.
    doc.mark_saved();
    assert!(!doc.modified());
    assert!(doc.begin_transaction().is_ok());
    assert!(doc
        .stage_vertex_positions(VertexPositionUpdate {
            mesh_id: MESH.to_string(),
            vertex_ids: vec![40],
            positions: vec![Vec2::new(-20.0, 25.0)],
        })
        .is_ok());
    assert!(doc
        .stage_vertex_positions(VertexPositionUpdate {
            mesh_id: MESH.to_string(),
            vertex_ids: vec![20],
            positions: vec![Vec2::new(20.0, 25.0)],
        })
        .is_ok());

    let transaction_revision = doc.revision();
    let edit = doc.commit_transaction();
    assert!(edit.status.is_ok() && edit.changes.kind == ChangeKind::Positions);
    assert_eq!(doc.revision(), transaction_revision + 1);
    assert!(doc.modified());
    assert_eq!(
        doc.get_mesh(MESH).unwrap().base_positions[0],
        Vec2::new(-20.0, 25.0)
    );
    assert_eq!(
        doc.get_mesh(MESH).unwrap().base_positions[3],
        Vec2::new(20.0, 25.0)
    );

    // Validation happens before any source write
    assert!(doc.begin_transaction().is_ok());
    assert!(doc
        .stage_vertex_positions(VertexPositionUpdate {
            mesh_id: MESH.to_string(),
            vertex_ids: vec![40],
            positions: vec![Vec2::new(1.0, 2.0)],
        })
        .is_ok());
    assert!(doc
        .stage_vertex_positions(VertexPositionUpdate {
            mesh_id: MESH.to_string(),
            vertex_ids: vec![999],
            positions: vec![Vec2::new(3.0, 4.0)],
        })
        .is_ok());

    let atomic_before = doc.get_mesh(MESH).unwrap().base_positions.clone();
    let transaction_revision = doc.revision();
    let edit = doc.commit_transaction();
    assert_eq!(edit.status.code, "MISSING_VERTEX");
    assert_eq!(doc.get_mesh(MESH).unwrap().base_positions, atomic_before);
    assert_eq!(doc.revision(), transaction_revision);

    assert!(doc.begin_transaction().is_ok());
    assert!(doc
        .stage_vertex_positions(VertexPositionUpdate {
            mesh_id: MESH.to_string(),
            vertex_ids: vec![40],
            positions: vec![Vec2::new(100.0, 100.0)],
        })
        .is_ok());
    let transaction_revision = doc.revision();
    assert!(doc.cancel_transaction().is_ok());
    assert_eq!(doc.get_mesh(MESH).unwrap().base_positions, atomic_before);
    assert_eq!(doc.revision(), transaction_revision);
    assert_eq!(doc.cancel_transaction().code, "NO_TRANSACTION");

    // Source restoration
    let checkpoint = doc.clone();
    assert!(doc
        .set_vertex_positions(MESH, &[10], &[Vec2::new(-11.0, -12.0)])
        .status
        .is_ok());
    let restore_revision = doc.revision();
    doc.restore_from(&checkpoint);
    assert_eq!(doc.revision(), restore_revision + 1);
    assert_eq!(
        doc.get_mesh(MESH).unwrap().base_positions,
        checkpoint.get_mesh(MESH).unwrap().base_positions
    );

    let mut replacement = doc.get_mesh(MESH).unwrap().clone();
    replacement.uvs[0] = Vec2::new(0.25, 0.5);
    assert!(doc.replace_mesh(replacement.clone()).status.is_ok());
    assert_eq!(doc.get_mesh(MESH).unwrap().uvs[0], Vec2::new(0.25, 0.5));

    replacement.triangles[0][0] = 999;
    assert!(!doc.replace_mesh(replacement).status.is_ok());
    assert_eq!(
        doc.get_mesh(MESH).unwrap().triangles,
        checkpoint.get_mesh(MESH).unwrap().triangles
    );

    let stale_revision = doc.revision() - 1;
    let stale_batch = vec![VertexPositionUpdate {
        mesh_id: MESH.to_string(),
        vertex_ids: vec![40],
        positions: vec![Vec2::new(5.0, 6.0)],
    }];
    let stale_before = doc.get_mesh(MESH).unwrap().base_positions.clone();
    assert_eq!(
        doc.apply_vertex_position_updates_at_revision(&stale_batch, stale_revision)
            .status
            .code,
        "STALE_REVISION"
    );
    assert_eq!(doc.get_mesh(MESH).unwrap().base_positions, stale_before);
    assert!(doc
        .apply_vertex_position_updates_at_revision(&stale_batch, doc.revision())
        .status
        .is_ok());
    assert_eq!(
        doc.get_mesh(MESH).unwrap().base_positions[0],
        Vec2::new(5.0, 6.0)
    );
}
