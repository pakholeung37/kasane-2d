use std::collections::HashMap;

use kasane_core::{Canvas, Vec2};
use kasane_sdk::{
    prepare_png_asset, rectangle_mesh, AuthoringSession, MeshProperties, TopologyReplacement,
};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000101";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000102";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000103";
const OTHER_ID: &str = "00000000-0000-4000-8000-000000000104";

fn fixture() -> AuthoringSession {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET_ID, "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "eye",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let mut session = AuthoringSession::new(
        DOCUMENT_ID,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    session
        .edit("fixture", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    session.drain_events();
    session
}

#[test]
fn explicit_replacements_preserve_geometry_and_identity() {
    let mut session = fixture();
    let original = session.mesh(MESH_ID).unwrap();
    let mut other = rectangle_mesh(
        OTHER_ID,
        "eye",
        ASSET_ID,
        Vec2::new(20.0, 20.0),
        Vec2::new(30.0, 30.0),
    )
    .unwrap();
    other.runtime_id = "runtime.other".into();
    session
        .edit("second mesh", None, |edit| edit.create_mesh(other))
        .unwrap();
    assert_eq!(session.find_meshes_by_name("eye").len(), 2);
    assert_eq!(
        session
            .require_unique_mesh("eye")
            .unwrap_err()
            .code
            .as_ref(),
        "AMBIGUOUS_NAME"
    );
    assert_eq!(
        session
            .require_unique_mesh("absent")
            .unwrap_err()
            .code
            .as_ref(),
        "NOT_FOUND"
    );

    let mut asset = session.asset(ASSET_ID).unwrap();
    asset.name = "renamed texture".into();
    let mut properties = MeshProperties::from(&original);
    properties.appearance.opacity = 0.4;
    properties.draw_order = Some(5.0);
    let (_, receipt) = session
        .edit("metadata and appearance", None, |edit| {
            edit.replace_asset(asset)?;
            edit.rename_mesh(OTHER_ID, "right eye")?;
            edit.update_mesh_properties(MESH_ID, properties)
        })
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(session.require_unique_mesh("eye").unwrap().id, MESH_ID);
    assert_eq!(session.asset(ASSET_ID).unwrap().name, "renamed texture");
    let changed = session.mesh(MESH_ID).unwrap();
    assert_eq!(changed.id, original.id);
    assert_eq!(changed.runtime_id, original.runtime_id);
    assert_eq!(changed.vertex_ids, original.vertex_ids);
    assert_eq!(changed.base_positions, original.base_positions);
    assert_eq!(changed.appearance.opacity, 0.4);
    assert_eq!(changed.draw_order, Some(5.0));
    session.undo().unwrap();
    assert_eq!(session.asset(ASSET_ID).unwrap().name, "texture");
    assert_eq!(session.mesh(MESH_ID).unwrap().appearance.opacity, 1.0);
    session.redo().unwrap();
    assert_eq!(session.mesh(MESH_ID).unwrap().appearance.opacity, 0.4);
}

#[test]
fn topology_requires_fresh_snapshot_complete_mapping_and_atomic_validation() {
    let mut session = fixture();
    let source = session.geometry(MESH_ID).unwrap();
    let mut replacement = session.mesh(MESH_ID).unwrap();
    replacement.vertex_ids = vec![10, 11, 12, 13];
    replacement.triangles = vec![[10, 11, 12], [10, 12, 13]];
    replacement.base_positions[0] = Vec2::new(-5.0, -5.0);
    let mapping = HashMap::from([(0, Some(10)), (1, Some(11)), (2, Some(12)), (3, Some(13))]);
    let make_update = || TopologyReplacement {
        mesh: replacement.clone(),
        binding: None,
        blend_bindings: Vec::new(),
        glues: Vec::new(),
        vertex_mapping: mapping.clone(),
    };
    let version = session.version();
    let mut edit = session
        .begin_edit("invalid mapping", Some(version))
        .unwrap();
    let mut incomplete = make_update();
    incomplete.vertex_mapping.remove(&0);
    assert_eq!(
        edit.replace_topology(&source, incomplete)
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_VERTEX_MAPPING"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(session.version(), version);
    assert_eq!(
        session.geometry(MESH_ID).unwrap().vertex_ids,
        source.vertex_ids
    );

    session
        .edit("topology", Some(source.version), |edit| {
            edit.replace_topology(&source, make_update())
        })
        .unwrap();
    assert_eq!(
        session.geometry(MESH_ID).unwrap().vertex_ids,
        vec![10, 11, 12, 13]
    );
    assert_eq!(
        session.mesh(MESH_ID).unwrap().triangles,
        replacement.triangles
    );
    let changed_version = session.version();
    let mut edit = session.begin_edit("stale topology", None).unwrap();
    assert_eq!(
        edit.replace_topology(&source, make_update())
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_TOPOLOGY"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(session.version(), changed_version);
    session.undo().unwrap();
    assert_eq!(
        session.geometry(MESH_ID).unwrap().vertex_ids,
        source.vertex_ids
    );
}

#[test]
fn invalid_property_replace_aborts_every_write_in_batch() {
    let mut session = fixture();
    let version = session.version();
    let mut properties = MeshProperties::from(&session.mesh(MESH_ID).unwrap());
    properties.texture_asset_id = OTHER_ID.into();
    let mut edit = session.begin_edit("bad texture", None).unwrap();
    edit.rename_mesh(MESH_ID, "changed").unwrap();
    assert_eq!(
        edit.update_mesh_properties(MESH_ID, properties)
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_ASSET"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(session.version(), version);
    assert_eq!(session.mesh(MESH_ID).unwrap().name, "eye");
    assert_eq!(session.drain_events().len(), 0);
}

#[test]
fn name_only_edit_preserves_evaluation_revision() {
    let mut session = fixture();
    let revision = session.version().revision;
    let evaluation_revision = session.evaluation_revision();
    session
        .edit("rename", None, |edit| edit.rename_mesh(MESH_ID, "new name"))
        .unwrap();
    assert_eq!(session.version().revision, revision + 1);
    assert_eq!(session.evaluation_revision(), evaluation_revision);
    assert_eq!(session.mesh(MESH_ID).unwrap().name, "new name");
    session.undo().unwrap();
    assert_eq!(session.mesh(MESH_ID).unwrap().name, "eye");
    assert_eq!(session.evaluation_revision(), session.version().revision);
}

#[test]
fn rectangle_grid_preserves_bound_surface_and_history() {
    use kasane_core::{BindingAxis, MeshBinding, MeshKeyform, Parameter};
    let mut session = fixture();
    let parameter_id = "00000000-0000-4000-8000-000000000105";
    let binding_id = "00000000-0000-4000-8000-000000000106";
    let corners = vec![
        Vec2::new(0., 0.),
        Vec2::new(12., -1.),
        Vec2::new(8., 12.),
        Vec2::new(-1., 11.),
    ];
    session
        .edit("binding", None, |edit| {
            edit.create_parameter(Parameter {
                id: parameter_id.into(),
                name: "pose".into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            edit.create_binding(MeshBinding {
                id: binding_id.into(),
                mesh_id: MESH_ID.into(),
                axes: vec![BindingAxis {
                    parameter_id: parameter_id.into(),
                    keys: vec![0., 1.],
                }],
                keyforms: vec![
                    MeshKeyform {
                        keys: vec![0.],
                        positions: vec![
                            Vec2::new(0., 0.),
                            Vec2::new(10., 0.),
                            Vec2::new(10., 10.),
                            Vec2::new(0., 10.),
                        ],
                        ..Default::default()
                    },
                    MeshKeyform {
                        keys: vec![1.],
                        positions: corners,
                        ..Default::default()
                    },
                ],
            })
        })
        .unwrap();
    let original = session.mesh(MESH_ID).unwrap();
    let version = session.version();
    let history = session.history_lengths();
    assert!(session.remesh_rectangle_grid(MESH_ID, 4, 3, None).is_err());
    assert_eq!(session.version(), version);
    assert_eq!(session.history_lengths(), history);
    let values = HashMap::from([(parameter_id.to_string(), 0.5)]);
    let before = session.evaluate(&values).unwrap();
    let receipt = session
        .remesh_rectangle_grid(MESH_ID, 4, 4, Some(version))
        .unwrap();
    assert!(receipt.changed);
    let mesh = session.mesh(MESH_ID).unwrap();
    assert_eq!(mesh.vertex_ids.len(), 25);
    assert_eq!(mesh.triangles.len(), 32);
    let after = session.evaluate(&values).unwrap();
    // Independently interpolate the old evaluated surface at every new vertex.
    let old = &before.drawables[0].positions;
    let new = &after.drawables[0].positions;
    for row in 0..=4 {
        for col in 0..=4 {
            let u = col as f32 / 4.;
            let v = row as f32 / 4.;
            let weights = if u >= v {
                [1. - u, u - v, v, 0.]
            } else {
                [1. - v, 0., u, v - u]
            };
            let x: f32 = old.iter().zip(weights).map(|(p, w)| p.x * w).sum();
            let y: f32 = old.iter().zip(weights).map(|(p, w)| p.y * w).sum();
            assert!((new[row * 5 + col].x - x).abs() < 1e-5);
            assert!((new[row * 5 + col].y - y).abs() < 1e-5);
        }
    }
    session.undo().unwrap();
    assert_eq!(session.mesh(MESH_ID).unwrap(), original);
    let current = session.version();
    assert_eq!(
        session
            .remesh_rectangle_grid(MESH_ID, 4, 4, Some(version))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_VERSION"
    );
    assert_eq!(session.version(), current);
    session.redo().unwrap();
    assert_eq!(session.mesh(MESH_ID).unwrap(), mesh);
}

#[test]
fn rectangle_grid_rejects_invalid_geometry_and_overflow() {
    use kasane_sdk::{rectangle_grid_geometry, MeshGeometry};
    let mesh = fixture().mesh(MESH_ID).unwrap();
    let mut source = MeshGeometry {
        vertex_ids: mesh.vertex_ids,
        positions: mesh.base_positions,
        uvs: mesh.uvs,
        triangles: mesh.triangles,
    };
    assert!(rectangle_grid_geometry(&source, usize::MAX, 1).is_err());
    assert!(rectangle_grid_geometry(&source, 0, 1).is_err());
    assert!(rectangle_grid_geometry(&source, 256, 256).is_err());
    let grid = rectangle_grid_geometry(&source, 4, 3).unwrap();
    assert_eq!(grid.vertex_ids.len(), 20);
    assert_eq!(grid.triangles.len(), 24);
    source.positions[0].x = f32::NAN;
    assert!(rectangle_grid_geometry(&source, 4, 4).is_err());
    source.positions[0].x = 0.;
    source.vertex_ids[3] = u32::MAX;
    source.triangles[1][2] = u32::MAX;
    assert!(rectangle_grid_geometry(&source, 2, 2).is_err());
    assert!(rectangle_grid_geometry(&source, 1, 1).is_ok());
}
