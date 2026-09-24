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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/asymmetric-2x2.png");
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
