use kasane_core::{Canvas, Document, ImageAsset, Mesh, Vec2};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000301";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000302";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000303";

fn document(extra_capacity: usize) -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            DOCUMENT_ID,
            Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)
        )
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            id: ASSET_ID.into(),
            name: "texture".into(),
            source: "/unused.png".into(),
            width: 2,
            height: 2,
            sha256: String::new(),
        })
        .status
        .is_ok());
    let mut positions = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(0.0, 10.0),
    ];
    positions.reserve_exact(extra_capacity);
    assert!(doc
        .create_mesh(Mesh {
            id: MESH_ID.into(),
            name: "mesh".into(),
            texture_asset_id: ASSET_ID.into(),
            vertex_ids: vec![0, 1, 2, 3],
            base_positions: positions,
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0)
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            ..Mesh::default()
        })
        .status
        .is_ok());
    doc
}

#[test]
fn estimates_spare_vertex_capacity_as_well_as_payload() {
    let tight = document(0);
    let reserved = document(1000);
    assert_eq!(tight.get_mesh(MESH_ID), reserved.get_mesh(MESH_ID));
    assert!(reserved.estimated_content_bytes() >= tight.estimated_content_bytes() + 1000 * 8);
    assert!(tight.checkpoint().estimated_bytes() > 0);
}
