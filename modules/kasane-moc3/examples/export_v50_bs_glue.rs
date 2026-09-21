use std::fs;
use std::path::PathBuf;

use kasane_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind, Canvas,
    DeltaGlueKeyform, DeltaKeyforms, Glue, GlueVertexPair, ImageAsset, Mesh, Parameter,
    ParameterKind, Vec2,
};
use kasane_core::Document;
use kasane_moc3::encode_moc3;

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let out_dir = root.join("tests/fixtures/external_v50_bs_glue");
    fs::create_dir_all(&out_dir).unwrap();

    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(320.0, 240.0),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    let _ = doc.add_asset(ImageAsset {
        id: id(2),
        name: "Texture0".to_string(),
        source: "textures/0.png".to_string(),
        width: 64,
        height: 64,
        sha256: "0".repeat(64),
    });

    let param_norm = Parameter {
        id: id(3),
        runtime_id: "ParamNorm".to_string(),
        name: "ParamNorm".to_string(),
        minimum: -1.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::Normal,
        repeat: false,
    };
    assert!(doc.create_parameter(param_norm).status.is_ok());

    let param_bs = Parameter {
        id: id(4),
        runtime_id: "ParamBS".to_string(),
        name: "ParamBS".to_string(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::BlendShape,
        repeat: false,
    };
    assert!(doc.create_parameter(param_bs).status.is_ok());

    let mesh_a = Mesh {
        id: id(5),
        runtime_id: "MeshA".to_string(),
        name: "MeshA".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
        ],
        triangles: vec![[1, 2, 3], [2, 4, 3]],
        ..Default::default()
    };
    assert!(doc.create_mesh(mesh_a).status.is_ok());

    let mesh_b = Mesh {
        id: id(6),
        runtime_id: "MeshB".to_string(),
        name: "MeshB".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![10, 20, 30, 40],
        base_positions: vec![
            Vec2::new(10.0, 20.0),
            Vec2::new(20.0, 20.0),
            Vec2::new(10.0, 30.0),
            Vec2::new(20.0, 30.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
        ],
        triangles: vec![[10, 20, 30], [20, 40, 30]],
        ..Default::default()
    };
    assert!(doc.create_mesh(mesh_b).status.is_ok());

    let glue = Glue {
        id: id(7),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: id(5),
        mesh_b_id: id(6),
        pairs: vec![GlueVertexPair {
            vertex_a: 1,
            vertex_b: 10,
            weight_a: 0.5,
            weight_b: 0.5,
        }],
        intensity: 0.2, // base intensity 0.2
        binding: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let bkt = BlendShapeKeyTable {
        id: id(8),
        parameter_id: id(4),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    let constraint = BlendShapeConstraint {
        id: id(9),
        parameter_id: id(3),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.5],
    };
    assert!(doc.create_blend_constraint(constraint).status.is_ok());

    let binding = BlendShapeBinding {
        id: id(10),
        target_id: id(7),
        target_kind: BlendShapeTargetKind::Glue,
        key_table_id: id(8),
        constraint_ids: vec![id(9)],
        keyforms: DeltaKeyforms::Glue(vec![
            DeltaGlueKeyform { intensity: 0.0 },
            DeltaGlueKeyform { intensity: 0.6 },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    let artifact = encode_moc3(&doc).expect("encode_moc3 failed");
    fs::write(out_dir.join("model.moc3"), &artifact.bytes).unwrap();

    let model3_json = r#"{
  "Version": 3,
  "FileReferences": {
    "Moc": "model.moc3",
    "Textures": [
      "texture_00.png"
    ]
  }
}
"#;
    fs::write(out_dir.join("model.model3.json"), model3_json).unwrap();
    println!(
        "Successfully exported external_v50_bs_glue to {}: {} bytes",
        out_dir.display(),
        artifact.bytes.len()
    );
}
