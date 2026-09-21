use kasane_core::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind, Canvas,
    DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, ImageAsset, Mesh, Parameter,
    ParameterKind, Vec2,
};
use kasane_core::Document;

const DOC: &str = "11111111-1111-4111-8111-111111111111";
const ASSET: &str = "22222222-2222-4222-8222-222222222222";
const MESH_A: &str = "33333333-3333-4333-8333-333333333333";
const MESH_B: &str = "44444444-4444-4444-8444-444444444444";
const PARAM_NORM: &str = "55555555-5555-4555-8555-555555555555";
const PARAM_BS: &str = "66666666-6666-4666-8666-666666666666";
const KEY_TABLE: &str = "77777777-7777-4777-8777-777777777777";
const CONSTRAINT: &str = "88888888-8888-4888-8888-888888888888";
const BINDING_BS: &str = "99999999-9999-4999-8999-999999999999";
const GLUE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

fn create_base_document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(DOC, Canvas::new(1000.0, 1000.0, Vec2::default(), 1.0))
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: ASSET.to_string(),
            name: "texture".to_string(),
            source: "textures/tex.png".to_string(),
            width: 100,
            height: 100,
            sha256: "0".repeat(64),
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .create_mesh(Mesh {
            id: MESH_A.to_string(),
            runtime_id: "MeshA".to_string(),
            name: "MeshA".to_string(),
            texture_asset_id: ASSET.to_string(),
            vertex_ids: vec![1, 2, 3],
            base_positions: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)],
            uvs: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .create_mesh(Mesh {
            id: MESH_B.to_string(),
            runtime_id: "MeshB".to_string(),
            name: "MeshB".to_string(),
            texture_asset_id: ASSET.to_string(),
            vertex_ids: vec![10, 20, 30],
            base_positions: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)],
            uvs: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)],
            triangles: vec![[10, 20, 30]],
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .create_parameter(Parameter {
            id: PARAM_NORM.to_string(),
            runtime_id: "ParamNorm".to_string(),
            name: "ParamNorm".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::Normal,
        })
        .status
        .is_ok());

    assert!(doc
        .create_parameter(Parameter {
            id: PARAM_BS.to_string(),
            runtime_id: "ParamBS".to_string(),
            name: "ParamBS".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::BlendShape,
        })
        .status
        .is_ok());

    doc
}

#[test]
fn test_blendshape_key_table_and_constraint_crud() {
    let mut doc = create_base_document();

    // Key table targeting normal parameter should fail
    let bad_table = BlendShapeKeyTable {
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_NORM.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(!doc.create_blend_key_table(bad_table).status.is_ok());

    // Valid key table
    let table = BlendShapeKeyTable {
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(table.clone()).status.is_ok());
    assert_eq!(doc.blend_key_table_order(), &[KEY_TABLE]);
    assert_eq!(doc.get_blend_key_table(KEY_TABLE), Some(&table));

    // Valid constraint
    let constraint = BlendShapeConstraint {
        id: CONSTRAINT.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![0.0, 1.0],
    };
    assert!(doc.create_blend_constraint(constraint.clone()).status.is_ok());
    assert_eq!(doc.blend_constraint_order(), &[CONSTRAINT]);
    assert_eq!(doc.get_blend_constraint(CONSTRAINT), Some(&constraint));

    // Erase parameter should fail because it's referenced by key table and constraint
    let erase_res = doc.erase_object(PARAM_BS);
    assert!(!erase_res.status.is_ok());
    assert!(erase_res.referrers.contains(&KEY_TABLE.to_string()));
    assert!(erase_res.referrers.contains(&CONSTRAINT.to_string()));
}

#[test]
fn test_blendshape_binding_and_mesh_topology_guard() {
    let mut doc = create_base_document();

    let table = BlendShapeKeyTable {
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(table).status.is_ok());

    let constraint = BlendShapeConstraint {
        id: CONSTRAINT.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![0.0, 1.0],
    };
    assert!(doc.create_blend_constraint(constraint).status.is_ok());

    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: MESH_A.to_string(),
        target_kind: BlendShapeTargetKind::Mesh,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![CONSTRAINT.to_string()],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)],
                ..Default::default()
            },
            DeltaMeshKeyform {
                positions: vec![Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(3.0, 0.0)],
                ..Default::default()
            },
        ]),
    };
    assert!(doc.create_blend_binding(binding.clone()).status.is_ok());
    assert_eq!(doc.blend_binding_order(), &[BINDING_BS]);
    assert_eq!(doc.get_blend_binding(BINDING_BS), Some(&binding));

    // Trying to replace mesh with different vertex count without updating keyforms must be rejected
    let mut modified_mesh = doc.get_mesh(MESH_A).unwrap().clone();
    modified_mesh.vertex_ids = vec![1, 2, 3, 4];
    modified_mesh.base_positions.push(Vec2::new(5.0, 5.0));
    modified_mesh.uvs.push(Vec2::new(0.5, 0.5));
    modified_mesh.triangles = vec![[1, 2, 3], [2, 3, 4]];
    let replace_res = doc.replace_mesh(modified_mesh);
    assert!(!replace_res.status.is_ok());
    assert_eq!(replace_res.status.code, "KEYFORMS_REQUIRED");
}

#[test]
fn test_glue_crud_and_references() {
    let mut doc = create_base_document();

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: MESH_A.to_string(),
        mesh_b_id: MESH_B.to_string(),
        pairs: vec![
            GlueVertexPair {
                vertex_a: 1,
                vertex_b: 10,
                weight_a: 0.5,
                weight_b: 0.5,
            },
            GlueVertexPair {
                vertex_a: 2,
                vertex_b: 20,
                weight_a: 0.5,
                weight_b: 0.5,
            },
        ],
        intensity: 1.0,
        binding_id: None,
    };
    assert!(doc.create_glue(glue.clone()).status.is_ok());
    assert_eq!(doc.glue_order(), &[GLUE_ID]);
    assert_eq!(doc.get_glue(GLUE_ID), Some(&glue));
    assert_eq!(doc.glues_for_mesh(MESH_A).len(), 1);

    // References: MESH_A cannot be erased while glued
    let erase_res = doc.erase_object(MESH_A);
    assert!(!erase_res.status.is_ok());
    assert!(erase_res.referrers.contains(&GLUE_ID.to_string()));

    // Mesh topology change that drops a glued vertex must be rejected
    let mut modified_mesh = doc.get_mesh(MESH_A).unwrap().clone();
    modified_mesh.vertex_ids = vec![2, 3, 4]; // vertex 1 dropped!
    modified_mesh.base_positions = vec![Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), Vec2::new(5.0, 5.0)];
    modified_mesh.uvs = vec![Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(0.5, 0.5)];
    modified_mesh.triangles = vec![[2, 3, 4]];
    let replace_res = doc.replace_mesh(modified_mesh);
    assert!(!replace_res.status.is_ok());
    assert_eq!(replace_res.status.code, "GLUE_CONFLICT");

    // Clean erase of glue
    assert!(doc.erase_object(GLUE_ID).status.is_ok());
    assert_eq!(doc.glue_order().len(), 0);
    assert_eq!(doc.get_glue(GLUE_ID), None);

    // Now mesh can be erased
    assert!(doc.erase_object(MESH_A).status.is_ok());
}

#[test]
fn test_blendshape_evaluation() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    let param_bs2 = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    assert!(doc
        .create_parameter(Parameter {
            id: param_bs2.to_string(),
            runtime_id: "ParamBS2".to_string(),
            name: "ParamBS2".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::BlendShape,
        })
        .status
        .is_ok());

    // Create blend key table on PARAM_BS (range 0.0..1.0, keys [0.0, 1.0], base_key = 0)
    let bkt = BlendShapeKeyTable {
        id: KEY_TABLE.to_string(),
        parameter_id: PARAM_BS.to_string(),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    assert!(doc.create_blend_key_table(bkt).status.is_ok());

    // Create a constraint on param_bs2 (range 0.0..1.0):
    // at 0.0 weight is 1.0, at 1.0 weight is 0.0
    let bsc = BlendShapeConstraint {
        id: CONSTRAINT.to_string(),
        parameter_id: param_bs2.to_string(),
        keys: vec![0.0, 1.0],
        weights: vec![1.0, 0.0],
    };
    assert!(doc.create_blend_constraint(bsc).status.is_ok());

    // Create blend binding for MESH_A:
    // base key (0.0): delta is zero
    // key 1 (1.0): dx = 5.0, dy = 10.0, draw_order = 50.0
    let binding = BlendShapeBinding {
        id: BINDING_BS.to_string(),
        target_id: MESH_A.to_string(),
        target_kind: BlendShapeTargetKind::Mesh,
        key_table_id: KEY_TABLE.to_string(),
        constraint_ids: vec![CONSTRAINT.to_string()],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![Vec2::default(); 3],
                ..Default::default()
            },
            DeltaMeshKeyform {
                positions: vec![Vec2::new(5.0, 10.0), Vec2::new(5.0, 10.0), Vec2::new(5.0, 10.0)],
                opacity: None,
                draw_order: Some(50.0),
                multiply: None,
                screen: None,
            },
        ]),
    };
    assert!(doc.create_blend_binding(binding).status.is_ok());

    // Case 1: PARAM_BS = 0.0 (base key) -> no delta
    let mut preview: PreviewValues = HashMap::new();
    preview.insert(PARAM_BS.to_string(), 0.0);
    preview.insert(param_bs2.to_string(), 0.0); // constraint weight = 1.0
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(0.0, 0.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(10.0, 0.0));
    assert_eq!(mesh_a.draw_order, 0);

    // Case 2: PARAM_BS = 1.0, constraint param_bs2 = 0.0 (weight 1.0) -> full delta (5, 10)
    // Note: doc.canvas().flag & 1 == 0 inverts Y for final output
    preview.insert(PARAM_BS.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(5.0, -10.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(15.0, -10.0));
    assert_eq!(mesh_a.draw_order, 50);

    // Case 3: PARAM_BS = 0.5, constraint = 0.0 (weight 1.0) -> half delta (2.5, 5.0)
    preview.insert(PARAM_BS.to_string(), 0.5);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(2.5, -5.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(12.5, -5.0));
    assert_eq!(mesh_a.draw_order, 25);

    // Case 4: PARAM_BS = 1.0, constraint param_bs2 = 0.5 (halfway between 0 and 1 -> weight 0.5)
    // Effective weight = 1.0 * 0.5 = 0.5 -> delta is (2.5, 5.0)
    preview.insert(PARAM_BS.to_string(), 1.0);
    preview.insert(param_bs2.to_string(), 0.5);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(2.5, -5.0));
    assert_eq!(mesh_a.positions[1], Vec2::new(12.5, -5.0));
    assert_eq!(mesh_a.draw_order, 25);

    // Case 5: PARAM_BS = 1.0, constraint param_bs2 = 1.0 (weight 0.0) -> 0 delta
    preview.insert(PARAM_BS.to_string(), 1.0);
    preview.insert(param_bs2.to_string(), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    assert_eq!(mesh_a.positions[0], Vec2::new(0.0, 0.0));
    assert_eq!(mesh_a.draw_order, 0);
}

#[test]
fn test_glue_evaluation() {
    use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
    use std::collections::HashMap;

    let mut doc = create_base_document();

    let mut mesh_b_mod = doc.get_mesh(MESH_B).unwrap().clone();
    mesh_b_mod.base_positions[0] = Vec2::new(10.0, 20.0);
    assert!(doc.replace_mesh(mesh_b_mod).status.is_ok());

    let glue = Glue {
        id: GLUE_ID.to_string(),
        runtime_id: "Glue0".to_string(),
        name: "Glue 0".to_string(),
        mesh_a_id: MESH_A.to_string(),
        mesh_b_id: MESH_B.to_string(),
        pairs: vec![
            GlueVertexPair {
                vertex_a: 1, // at (0, 0)
                vertex_b: 10, // at (10, 20)
                weight_a: 0.5,
                weight_b: 0.5,
            },
        ],
        intensity: 1.0,
        binding_id: None,
    };
    assert!(doc.create_glue(glue).status.is_ok());

    let mut frame = DrawableFrame::default();
    let preview = HashMap::new();
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());

    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let mesh_b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();

    // d = (10 - 0, 20 - 0) = (10, 20)
    // p_a = (0, 0) + (10, 20) * 0.5 = (5, 10) -> canvas y inverted: (5, -10)
    // p_b = (10, 20) - (10, 20) * 0.5 = (5, 10) -> canvas y inverted: (5, -10)
    assert_eq!(mesh_a.positions[0], Vec2::new(5.0, -10.0));
    assert_eq!(mesh_b.positions[0], Vec2::new(5.0, -10.0));

    // Test with intensity 0.5
    let mut glue_half = doc.get_glue(GLUE_ID).unwrap().clone();
    glue_half.intensity = 0.5;
    assert!(doc.replace_glue(glue_half).status.is_ok());

    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let mesh_a = frame.drawables.iter().find(|d| d.id == MESH_A).unwrap();
    let mesh_b = frame.drawables.iter().find(|d| d.id == MESH_B).unwrap();

    // p_a = (0, 0) + (10, 20) * (0.5 * 0.5) = (2.5, 5.0) -> canvas y inverted: (2.5, -5.0)
    // p_b = (10, 20) - (10, 20) * (0.5 * 0.5) = (7.5, 15.0) -> canvas y inverted: (7.5, -15.0)
    assert_eq!(mesh_a.positions[0], Vec2::new(2.5, -5.0));
    assert_eq!(mesh_b.positions[0], Vec2::new(7.5, -15.0));
}


#[test]
fn parameter_edits_validate_blend_dependents_atomically() {
    let mut doc = create_base_document();
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    let before = doc.clone();
    let mut p = doc.get_parameter(PARAM_BS).unwrap().clone();
    p.maximum = 0.5;
    assert!(!doc.replace_parameter(p).status.is_ok());
    assert!(doc.same_content(&before));
    let mut p = doc.get_parameter(PARAM_BS).unwrap().clone();
    p.kind = ParameterKind::Normal;
    assert!(!doc.replace_parameter(p).status.is_ok());
    assert!(doc.same_content(&before));

    // Constraints may reference normal parameters, as the Core does.
    assert!(doc
        .create_blend_constraint(BlendShapeConstraint {
            id: CONSTRAINT.into(),
            parameter_id: PARAM_NORM.into(),
            keys: vec![-1.0, 1.0],
            weights: vec![1.0, 0.0],
        })
        .status
        .is_ok());
    let before = doc.clone();
    let mut p = doc.get_parameter(PARAM_NORM).unwrap().clone();
    p.minimum = 0.0;
    assert!(!doc.replace_parameter(p).status.is_ok());
    assert!(doc.same_content(&before));
}

#[test]
fn warp_grid_edits_validate_blend_dependents_atomically() {
    use kasane_core::types::{DeltaWarpKeyform, Transform, TransformKind};
    let mut doc = create_base_document();
    let warp_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    assert!(doc
        .create_transform(Transform {
            id: warp_id.into(),
            runtime_id: "Warp".into(),
            kind: TransformKind::Warp,
            rows: 1,
            columns: 1,
            points: vec![Vec2::default(); 4],
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            id: KEY_TABLE.into(),
            parameter_id: PARAM_BS.into(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
            id: BINDING_BS.into(),
            target_id: warp_id.into(),
            target_kind: BlendShapeTargetKind::Warp,
            key_table_id: KEY_TABLE.into(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Warp(vec![
                DeltaWarpKeyform {
                    points: vec![Vec2::default(); 4],
                    ..Default::default()
                };
                2
            ]),
        })
        .status
        .is_ok());
    let before = doc.clone();
    let mut warp = doc.get_transform(warp_id).unwrap().clone();
    warp.rows = 2;
    warp.points = vec![Vec2::default(); 6];
    assert!(!doc.replace_transform(warp).status.is_ok());
    assert!(doc.same_content(&before));
    let mut warp = doc.get_transform(warp_id).unwrap().clone();
    warp.kind = TransformKind::Rotation;
    assert!(!doc.replace_transform(warp).status.is_ok());
    assert!(doc.same_content(&before));
}
