use std::collections::HashMap;

use kasane_core::{
    evaluate_frame, BindingAxis, Canvas, DeltaKeyforms, DeltaMeshKeyform, Document,
    DrawableFrame, ImageAsset, Mesh, MeshBinding, MeshKeyform, Parameter, ParameterKind,
    Vec2,
};
use kasane_core::types::{BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind};

fn id(n: i32) -> String {
    format!("{:08x}-1111-4111-8111-111111111111", n)
}

fn create_cyclic_fixture() -> (Document, String, String) {
    let mut d = Document::new();
    assert!(d
        .initialize(
            id(1),
            Canvas::new(200.0, 200.0, Vec2::new(100.0, 100.0), 100.0)
        )
        .is_ok());

    assert!(d
        .add_asset(ImageAsset {
            id: id(2),
            name: "asset".to_string(),
            source: "memory://unloaded".to_string(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());

    let mesh_id = id(3);
    let m = Mesh {
        id: mesh_id.clone(),
        name: "mesh".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(50.0, 100.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ],
        triangles: vec![[1, 2, 3]],
        runtime_id: "CyclicMesh".to_string(),
        ..Default::default()
    };
    assert!(d.create_mesh(m).status.is_ok());

    let param_id = id(4);
    let p = Parameter {
        id: param_id.clone(),
        name: "ParamAngle".to_string(),
        runtime_id: "ParamAngle".to_string(),
        minimum: -1.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 4,
        kind: ParameterKind::Normal,
        repeat: true,
    };
    assert!(d.create_parameter(p).status.is_ok());

    // Binding with 3 keys: [-1.0, 0.0, 1.0]
    let binding_id = id(5);
    let b = MeshBinding {
        id: binding_id,
        mesh_id: mesh_id.clone(),
        axes: vec![BindingAxis {
            parameter_id: param_id.clone(),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: vec![
            MeshKeyform {
                keys: vec![-1.0],
                positions: vec![
                    Vec2::new(-20.0, 0.0),
                    Vec2::new(80.0, 0.0),
                    Vec2::new(30.0, 100.0),
                ],
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![0.0],
                positions: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(100.0, 0.0),
                    Vec2::new(50.0, 100.0),
                ],
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![1.0],
                positions: vec![
                    Vec2::new(20.0, 0.0),
                    Vec2::new(120.0, 0.0),
                    Vec2::new(70.0, 100.0),
                ],
                ..Default::default()
            },
        ],
    };
    assert!(d.create_binding(b).status.is_ok());

    (d, mesh_id, param_id)
}

#[test]
fn test_cyclic_min_max_and_boundaries() {
    let (d, _, param_id) = create_cyclic_fixture();
    let mut frame = DrawableFrame::default();

    // 1. Min boundary: -1.0
    let mut preview = HashMap::new();
    preview.insert(param_id.clone(), -1.0);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert_eq!(eval_param.requested, -1.0);
    assert_eq!(eval_param.value, -1.0);
    assert!(!eval_param.clamped, "Repeat parameters must never flag clamped: true");

    // 2. Center: 0.0
    preview.insert(param_id.clone(), 0.0);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert_eq!(eval_param.requested, 0.0);
    assert_eq!(eval_param.value, 0.0);
    assert!(!eval_param.clamped);

    // 3. Max boundary: 1.0 (Live2D wrap formula wraps range_max to range_min)
    preview.insert(param_id.clone(), 1.0);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert_eq!(eval_param.requested, 1.0);
    assert_eq!(eval_param.value, -1.0, "1.0 at upper bound wraps to -1.0");
    assert!(!eval_param.clamped);
}

#[test]
fn test_cyclic_epsilon_sides() {
    let (d, _, param_id) = create_cyclic_fixture();
    let mut frame = DrawableFrame::default();
    let eps = 1e-5f32;

    let mut preview = HashMap::new();

    // 1. max - eps: close to 1.0, no wrap
    preview.insert(param_id.clone(), 1.0 - eps);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert!((eval_param.value - (1.0 - eps)).abs() < 1e-6);

    // 2. max + eps: just past 1.0, wraps to -1.0 + eps
    preview.insert(param_id.clone(), 1.0 + eps);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert!((eval_param.value - (-1.0 + eps)).abs() < 1e-6);

    // 3. min - eps: just below -1.0, wraps to 1.0 - eps
    preview.insert(param_id.clone(), -1.0 - eps);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert!((eval_param.value - (1.0 - eps)).abs() < 1e-6);

    // 4. min + eps: just above -1.0, no wrap
    preview.insert(param_id.clone(), -1.0 + eps);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    let eval_param = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
    assert!((eval_param.value - (-1.0 + eps)).abs() < 1e-6);
}

#[test]
fn test_cyclic_multi_period_positive_negative() {
    let (d, _, param_id) = create_cyclic_fixture();
    let mut base_frame = DrawableFrame::default();

    // Base value at 0.5
    let mut preview = HashMap::new();
    preview.insert(param_id.clone(), 0.5);
    assert!(evaluate_frame(&d, &preview, &mut base_frame).is_ok());
    let base_positions = base_frame.drawables[0].positions.clone();

    // Equivalent values across multiple positive and negative periods (period = 2.0)
    let test_values = [
        0.5 + 1.0 * 2.0,  // +1 period: 2.5
        0.5 + 2.0 * 2.0,  // +2 periods: 4.5
        0.5 + 5.0 * 2.0,  // +5 periods: 10.5
        0.5 - 1.0 * 2.0,  // -1 period: -1.5
        0.5 - 2.0 * 2.0,  // -2 periods: -3.5
        0.5 - 5.0 * 2.0,  // -5 periods: -9.5
    ];

    for &val in &test_values {
        let mut frame = DrawableFrame::default();
        preview.insert(param_id.clone(), val);
        assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
        let p = frame.parameters.iter().find(|p| p.id == param_id).unwrap();
        assert!((p.value - 0.5).abs() < 1e-5, "val {} wrapped to {}", val, p.value);
        assert_eq!(p.requested, val);
        assert!(!p.clamped);

        for (v1, v2) in base_positions.iter().zip(frame.drawables[0].positions.iter()) {
            assert!((v1.x - v2.x).abs() < 1e-5);
            assert!((v1.y - v2.y).abs() < 1e-5);
        }
    }
}

#[test]
fn test_cyclic_fixed_param_repeated_frames() {
    let (d, _, param_id) = create_cyclic_fixture();
    let mut preview = HashMap::new();
    preview.insert(param_id.clone(), 0.75);

    let mut first_frame = DrawableFrame::default();
    assert!(evaluate_frame(&d, &preview, &mut first_frame).is_ok());

    // 100 consecutive frames with identical preview
    for _ in 0..100 {
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
        assert_eq!(frame.parameters[0].value, first_frame.parameters[0].value);
        assert_eq!(frame.drawables[0].positions, first_frame.drawables[0].positions);
    }
}

#[test]
fn test_cyclic_seam_a_b_a() {
    let (d, _, param_id) = create_cyclic_fixture();
    let mut preview = HashMap::new();

    // Animate forward crossing the seam: 0.90 -> 0.95 -> 1.05 (wraps to -0.95) -> 1.10 (wraps to -0.90)
    // Then reverse back to 0.90: A -> B -> A
    let forward_steps = [0.90, 0.95, 0.99, 1.01, 1.05, 1.10];
    let mut forward_frames = Vec::new();

    for &val in &forward_steps {
        let mut frame = DrawableFrame::default();
        preview.insert(param_id.clone(), val);
        assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
        forward_frames.push(frame);
    }

    // Step backwards through the exact same values
    let mut reverse_steps = forward_steps.to_vec();
    reverse_steps.reverse();

    for (step_idx, &val) in reverse_steps.iter().enumerate() {
        let mut frame = DrawableFrame::default();
        preview.insert(param_id.clone(), val);
        assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());

        let forward_idx = forward_steps.len() - 1 - step_idx;
        let expected = &forward_frames[forward_idx];

        assert_eq!(frame.parameters[0].value, expected.parameters[0].value);
        assert_eq!(frame.drawables[0].positions, expected.drawables[0].positions);
    }
}

#[test]
fn test_cyclic_constraint_and_blendshape_combination() {
    let (mut d, mesh_id, cyclic_param_id) = create_cyclic_fixture();

    // Add BlendShape parameter
    let bs_param_id = id(10);
    assert!(d
        .create_parameter(Parameter {
            id: bs_param_id.clone(),
            runtime_id: "ParamMouthOpen".to_string(),
            name: "Mouth Open".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            kind: ParameterKind::BlendShape,
            repeat: false,
        })
        .status
        .is_ok());

    // BlendShapeKeyTable on bs_param_id
    let kt_id = id(11);
    assert!(d
        .create_blend_key_table(BlendShapeKeyTable {
            id: kt_id.clone(),
            parameter_id: bs_param_id.clone(),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })
        .status
        .is_ok());

    // Constraint driven by the CYCLIC parameter!
    let c_id = id(12);
    assert!(d
        .create_blend_constraint(BlendShapeConstraint {
            id: c_id.clone(),
            parameter_id: cyclic_param_id.clone(),
            keys: vec![-1.0, 0.0, 1.0],
            weights: vec![0.0, 1.0, 0.0],
        })
        .status
        .is_ok());

    // BlendShapeBinding on the mesh constrained by the cyclic parameter
    let bb_id = id(13);
    assert!(d
        .create_blend_binding(BlendShapeBinding {
            id: bb_id,
            target_id: mesh_id,
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: kt_id,
            constraint_ids: vec![c_id],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(10.0, 10.0), Vec2::new(10.0, 10.0), Vec2::new(10.0, 10.0)],
                    opacity: None,
                    draw_order: None,
                    multiply: None,
                    screen: None,
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(50.0, 50.0), Vec2::new(50.0, 50.0), Vec2::new(50.0, 50.0)],
                    opacity: None,
                    draw_order: None,
                    multiply: None,
                    screen: None,
                },
            ]),
        })
        .status
        .is_ok());

    // When cyclic parameter is at 0.0 (weight 1.0 in constraint), delta applies at full strength
    let mut preview = HashMap::new();
    preview.insert(bs_param_id.clone(), 1.0);
    preview.insert(cyclic_param_id.clone(), 0.0);

    let mut frame_base = DrawableFrame::default();
    assert!(evaluate_frame(&d, &preview, &mut frame_base).is_ok());

    // When cyclic parameter is at +1 period (2.0) or -1 period (-2.0), wraps to 0.0, yielding identical constraint!
    for &val in &[2.0, 4.0, -2.0, -4.0] {
        let mut frame = DrawableFrame::default();
        preview.insert(cyclic_param_id.clone(), val);
        assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
        assert_eq!(frame.drawables[0].positions, frame_base.drawables[0].positions);
    }

    // When cyclic parameter is at 1.0 (weight 0.0 in constraint), delta is gated to zero
    preview.insert(cyclic_param_id.clone(), 1.0); // wraps to -1.0, where constraint weight is 0.0
    let mut frame_gated = DrawableFrame::default();
    assert!(evaluate_frame(&d, &preview, &mut frame_gated).is_ok());
    assert_ne!(frame_gated.drawables[0].positions, frame_base.drawables[0].positions);
}

#[test]
fn test_cyclic_rejects_non_finite_and_invalid_ranges() {
    let (d, _, param_id) = create_cyclic_fixture();
    let mut frame = DrawableFrame::default();

    for bad_val in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut preview = HashMap::new();
        preview.insert(param_id.clone(), bad_val);
        let err = evaluate_frame(&d, &preview, &mut frame);
        assert!(!err.is_ok());
        assert_eq!(err.code, "NON_FINITE");
    }
}
