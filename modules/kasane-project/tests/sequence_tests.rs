use std::collections::{HashMap, HashSet};

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    BindingAxis, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform, Parameter, Part, RotationPose,
    Transform, TransformKind, Vec2, VertexPositionUpdate,
};
use kasane_core::Document;
use kasane_moc3::encode_moc3;
use kasane_project::codec::{decode_project, encode_project};

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u64() as usize % (max - min + 1))
    }

    fn next_f32(&mut self, min: f32, max: f32) -> f32 {
        let t = (self.next_u64() & 0xFFFF_FFFF) as f32 / 4294967295.0;
        min + t * (max - min)
    }

    fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }

    fn choose<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            Some(&slice[self.next_range(0, slice.len() - 1)])
        }
    }
}

fn make_uuid(kind: u16, id: u32) -> String {
    format!("{kind:04x}{id:04x}-4111-4111-8111-111111111111")
}

fn build_initial_document() -> (Document, Vec<String>) {
    let mut doc = Document::new();
    let doc_id = make_uuid(0x0001, 1);
    assert!(doc
        .initialize(
            doc_id,
            Canvas::new(1280.0, 720.0, Vec2::new(640.0, 360.0), 100.0),
        )
        .is_ok());

    let asset1 = make_uuid(0x0002, 1);
    let asset2 = make_uuid(0x0002, 2);
    let fake_sha256 =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
    assert!(doc
        .add_asset(ImageAsset {
            id: asset1.clone(),
            name: "texture_0".to_string(),
            source: "assets/0.png".to_string(),
            sha256: fake_sha256.clone(),
            width: 512,
            height: 512,
        })
        .status
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            id: asset2.clone(),
            name: "texture_1".to_string(),
            source: "assets/1.png".to_string(),
            sha256: fake_sha256,
            width: 512,
            height: 512,
        })
        .status
        .is_ok());

    let mut param_ids = Vec::new();
    for i in 0..2 {
        let pid = make_uuid(0x0003, i + 1);
        param_ids.push(pid.clone());
        assert!(doc
            .create_parameter(Parameter {
                id: pid.clone(),
                runtime_id: format!("Param_{i}"),
                name: format!("Parameter {i}"),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                decimal_places: 4,
                ..Default::default()
            })
            .status
            .is_ok());
    }

    let root_part = make_uuid(0x0004, 1);
    assert!(doc
        .create_part(Part {
            id: root_part.clone(),
            runtime_id: "RootPart".to_string(),
            name: "Root Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 0.0,
        })
        .status
        .is_ok());

    let rot_id = make_uuid(0x0005, 1);
    assert!(doc
        .create_transform(Transform {
            id: rot_id.clone(),
            runtime_id: "RotTransform".to_string(),
            name: "Rot Transform".to_string(),
            part_id: root_part.clone(),
            parent_id: String::new(),
            kind: TransformKind::Rotation,
            rotation: RotationPose {
                origin: Vec2::new(640.0, 360.0).into(),
                angle: 0.0,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
            ..Default::default()
        })
        .status
        .is_ok());

    let warp_id = make_uuid(0x0005, 2);
    let mut warp = Transform {
        id: warp_id.clone(),
        runtime_id: "WarpTransform".to_string(),
        name: "Warp Transform".to_string(),
        part_id: root_part.clone(),
        parent_id: rot_id.clone(),
        kind: TransformKind::Warp,
        rows: 2,
        columns: 2,
        quad: true,
        points: Vec::new(),
        ..Default::default()
    };
    for r in 0..=2 {
        for c in 0..=2 {
            warp.points
                .push(Vec2::new(c as f32 * 50.0 - 50.0, r as f32 * 50.0 - 50.0));
        }
    }
    assert!(doc.create_transform(warp).status.is_ok());

    let mesh_id = make_uuid(0x0006, 1);
    let mesh = Mesh {
        id: mesh_id.clone(),
        runtime_id: "Mesh_1".to_string(),
        name: "Mesh 1".to_string(),
        texture_asset_id: asset1,
        part_id: root_part,
        deformer_id: warp_id,
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(-20.0, 20.0),
            Vec2::new(-20.0, -20.0),
            Vec2::new(20.0, -20.0),
            Vec2::new(20.0, 20.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        ],
        triangles: vec![[1, 2, 3], [1, 3, 4]],
        draw_order: Some(1.0),
        ..Default::default()
    };
    assert!(doc.create_mesh(mesh).status.is_ok());

    let binding_id = make_uuid(0x0007, 1);
    let binding = MeshBinding {
        id: binding_id,
        mesh_id: mesh_id.clone(),
        axes: vec![BindingAxis {
            parameter_id: param_ids[0].clone(),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: vec![
            MeshKeyform {
                keys: vec![-1.0],
                positions: vec![
                    Vec2::new(-25.0, 20.0),
                    Vec2::new(-25.0, -20.0),
                    Vec2::new(15.0, -20.0),
                    Vec2::new(15.0, 20.0),
                ],
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![0.0],
                positions: vec![
                    Vec2::new(-20.0, 20.0),
                    Vec2::new(-20.0, -20.0),
                    Vec2::new(20.0, -20.0),
                    Vec2::new(20.0, 20.0),
                ],
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![1.0],
                positions: vec![
                    Vec2::new(-15.0, 20.0),
                    Vec2::new(-15.0, -20.0),
                    Vec2::new(25.0, -20.0),
                    Vec2::new(25.0, 20.0),
                ],
                ..Default::default()
            },
        ],
    };
    assert!(doc.create_binding(binding).status.is_ok());

    doc.mark_saved();
    (doc, param_ids)
}

fn verify_invariants(doc: &Document, label: &str) {
    // Invariant 1: Parts hierarchy is a valid DAG (no cycles, no dangling parent)
    for p_id in doc.part_order() {
        let p = doc.get_part(p_id).expect("part in order must exist");
        if !p.parent_id.is_empty() {
            assert!(
                doc.get_part(&p.parent_id).is_some(),
                "Dangling part parent: {p_id} -> {}",
                p.parent_id
            );
        }
        let mut visited = HashSet::new();
        visited.insert(p_id.clone());
        let mut curr = p.parent_id.clone();
        while !curr.is_empty() {
            assert!(
                visited.insert(curr.clone()),
                "Cycle detected in part hierarchy starting from {p_id} at {label}"
            );
            curr = doc
                .get_part(&curr)
                .expect("ancestor must exist")
                .parent_id
                .clone();
        }
    }

    // Invariant 2: Transform hierarchy is a valid DAG
    for t_id in doc.transform_order() {
        let t = doc
            .get_transform(t_id)
            .expect("transform in order must exist");
        assert!(
            doc.get_part(&t.part_id).is_some(),
            "Transform {t_id} points to missing part {}",
            t.part_id
        );
        if !t.parent_id.is_empty() {
            assert!(
                doc.get_transform(&t.parent_id).is_some(),
                "Transform {t_id} points to missing transform parent {}",
                t.parent_id
            );
        }
        let mut visited = HashSet::new();
        visited.insert(t_id.clone());
        let mut curr = t.parent_id.clone();
        while !curr.is_empty() {
            assert!(
                visited.insert(curr.clone()),
                "Cycle detected in transform hierarchy starting from {t_id} at {label}"
            );
            curr = doc
                .get_transform(&curr)
                .expect("transform ancestor must exist")
                .parent_id
                .clone();
        }
    }

    // Invariant 3: Mesh integrity
    for m_id in doc.mesh_order() {
        let m = doc.get_mesh(m_id).expect("mesh in order must exist");
        assert!(
            doc.get_asset(&m.texture_asset_id).is_some(),
            "Mesh {m_id} has invalid asset {}",
            m.texture_asset_id
        );
        assert!(
            doc.get_part(&m.part_id).is_some(),
            "Mesh {m_id} has invalid part {}",
            m.part_id
        );
        if !m.deformer_id.is_empty() {
            assert!(
                doc.get_transform(&m.deformer_id).is_some(),
                "Mesh {m_id} has invalid deformer {}",
                m.deformer_id
            );
        }
        for mask_id in &m.masks {
            assert!(
                doc.get_mesh(mask_id).is_some(),
                "Mesh {m_id} has invalid mask {mask_id}"
            );
        }
        assert_eq!(
            m.vertex_ids.len(),
            m.base_positions.len(),
            "Mesh {m_id} vertex/position count mismatch"
        );
        assert_eq!(
            m.vertex_ids.len(),
            m.uvs.len(),
            "Mesh {m_id} vertex/uv count mismatch"
        );
    }

    // Invariant 4: Parameter & Bindings integrity
    for b_id in doc.binding_order() {
        let b = doc.get_binding(b_id).expect("binding in order must exist");
        let mesh = doc
            .get_mesh(&b.mesh_id)
            .expect("binding target mesh must exist");
        let mut total_keys = 1usize;
        for axis in &b.axes {
            let param = doc
                .get_parameter(&axis.parameter_id)
                .expect("binding axis param must exist");
            total_keys *= axis.keys.len();
            for &k in &axis.keys {
                assert!(
                    k >= param.minimum && k <= param.maximum,
                    "Binding axis key {k} out of bounds [{min}, {max}] at {label}",
                    min = param.minimum,
                    max = param.maximum
                );
            }
        }
        assert_eq!(
            b.keyforms.len(),
            total_keys,
            "Binding {b_id} keyform count mismatch"
        );
        for kf in &b.keyforms {
            assert_eq!(
                kf.positions.len(),
                mesh.base_positions.len(),
                "Keyform position count mismatch with mesh"
            );
        }
    }

    // Invariant 5: Scene Bindings integrity
    for sb_id in doc.scene_binding_order() {
        let sb = doc
            .get_scene_binding(sb_id)
            .expect("scene binding in order must exist");
        assert!(
            doc.get_transform(&sb.target_id).is_some() || doc.get_part(&sb.target_id).is_some(),
            "Scene binding target {} missing",
            sb.target_id
        );
        let mut total_keys = 1usize;
        for axis in &sb.axes {
            let param = doc
                .get_parameter(&axis.parameter_id)
                .expect("scene binding axis param must exist");
            total_keys *= axis.keys.len();
            for &k in &axis.keys {
                assert!(
                    k >= param.minimum && k <= param.maximum,
                    "Scene binding axis key out of bounds at {label}"
                );
            }
        }
        assert_eq!(
            sb.keyforms.len(),
            total_keys,
            "Scene binding keyform count mismatch"
        );
    }

    // Invariant 6: Persistence roundtrip
    let encoded = encode_project(doc).expect("encode_project must succeed on valid document");
    let decoded = decode_project(&encoded).expect("decode_project must succeed on encoded project");
    assert!(
        doc.same_content(&decoded),
        "Persistence roundtrip decoded document does not match source content at {label}"
    );

    // Invariant 7: Preview isolation
    let rev_before = doc.revision();
    let mod_before = doc.modified();
    let mut preview_vals = HashMap::new();
    for pid in doc.parameter_order() {
        preview_vals.insert(pid.clone(), 0.5);
    }
    let mut frame = DrawableFrame::default();
    let eval_status = evaluate_frame(doc, &preview_vals, &mut frame);
    assert!(
        eval_status.is_ok(),
        "evaluate_frame must succeed at {label}: {:?}",
        eval_status
    );
    assert_eq!(
        doc.revision(),
        rev_before,
        "evaluate_frame must not alter revision at {label}"
    );
    assert_eq!(
        doc.modified(),
        mod_before,
        "evaluate_frame must not alter modified flag at {label}"
    );

    // Invariant 8: MOC3 exportability
    if !doc.mesh_order().is_empty() {
        let moc_res = encode_moc3(doc);
        assert!(
            moc_res.is_ok(),
            "encode_moc3 failed at {label}: {:?}",
            moc_res.err()
        );
        let artifact = moc_res.unwrap();
        assert!(
            artifact.bytes.starts_with(b"MOC3"),
            "Exported MOC3 header invalid at {label}"
        );
    }
}

#[test]
fn test_random_operation_sequences_and_invariants() {
    for seed in [0x1234_5678_u64, 0x9ABC_DEF0_u64, 0xCAFE_BABE_u64] {
        let mut rng = Rng::new(seed);
        let (mut doc, _) = build_initial_document();
        let mut id_counter = 100u32;

        let mut tx_snapshot: Option<Document> = None;

        for step in 0..150 {
            let label = format!("seed={seed:#x}, step={step}");
            let doc_before = doc.clone();
            let rev_before = doc.revision();
            let mod_before = doc.modified();

            let was_transaction_active = doc.transaction_active();
            let op_type = rng.next_range(0, 17);
            let mut edit_result = None;

            match op_type {
                0 => {
                    // Create Part
                    id_counter += 1;
                    let new_id = make_uuid(0x0004, id_counter);
                    let parent_id = if rng.next_bool() && !doc.part_order().is_empty() {
                        rng.choose(doc.part_order()).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let res = doc.create_part(Part {
                        id: new_id,
                        runtime_id: format!("Part_{id_counter}"),
                        name: format!("Part {id_counter}"),
                        parent_id,
                        enabled: true,
                        draw_order: rng.next_f32(-10.0, 10.0),
                    });
                    edit_result = Some(res.status);
                }
                1 => {
                    // Reparent Part
                    if !doc.part_order().is_empty() {
                        let part_id = rng.choose(doc.part_order()).cloned().unwrap();
                        let mut p = doc.get_part(&part_id).unwrap().clone();
                        p.parent_id = if rng.next_bool() && doc.part_order().len() > 1 {
                            rng.choose(doc.part_order()).cloned().unwrap()
                        } else {
                            String::new()
                        };
                        let res = doc.replace_part(p);
                        edit_result = Some(res.status);
                    }
                }
                2 => {
                    // Create Transform (Rotation)
                    id_counter += 1;
                    let new_id = make_uuid(0x0005, id_counter);
                    let part_id = rng.choose(doc.part_order()).cloned().unwrap();
                    let parent_id = if rng.next_bool() && !doc.transform_order().is_empty() {
                        rng.choose(doc.transform_order()).cloned().unwrap()
                    } else {
                        String::new()
                    };
                    let res = doc.create_transform(Transform {
                        id: new_id,
                        runtime_id: format!("Rot_{id_counter}"),
                        name: format!("Rot {id_counter}"),
                        part_id,
                        parent_id,
                        kind: TransformKind::Rotation,
                        rotation: RotationPose {
                            origin: Vec2::new(rng.next_f32(0.0, 600.0), rng.next_f32(0.0, 400.0))
                                .into(),
                            angle: rng.next_f32(-45.0, 45.0),
                            scale: rng.next_f32(0.8, 1.2),
                            reflect_x: false,
                            reflect_y: false,
                        },
                        ..Default::default()
                    });
                    edit_result = Some(res.status);
                }
                3 => {
                    // Reparent Transform (including cyclic check)
                    if !doc.transform_order().is_empty() {
                        let t_id = rng.choose(doc.transform_order()).cloned().unwrap();
                        let mut t = doc.get_transform(&t_id).unwrap().clone();
                        t.parent_id = if rng.next_bool() && doc.transform_order().len() > 1 {
                            rng.choose(doc.transform_order()).cloned().unwrap()
                        } else {
                            String::new()
                        };
                        let res = doc.replace_transform(t);
                        edit_result = Some(res.status);
                    }
                }
                4 => {
                    // Create Parameter
                    id_counter += 1;
                    let new_id = make_uuid(0x0003, id_counter);
                    let min = rng.next_f32(-2.0, 0.0);
                    let max = rng.next_f32(0.0, 2.0);
                    let res = doc.create_parameter(Parameter {
                        id: new_id,
                        runtime_id: format!("Param_{id_counter}"),
                        name: format!("Parameter {id_counter}"),
                        minimum: min,
                        maximum: max,
                        default_value: (min + max) * 0.5,
                        decimal_places: 4,
                        ..Default::default()
                    });
                    edit_result = Some(res.status);
                }
                5 => {
                    // Modify Parameter Range
                    if !doc.parameter_order().is_empty() {
                        let pid = rng.choose(doc.parameter_order()).cloned().unwrap();
                        let mut p = doc.get_parameter(&pid).unwrap().clone();
                        if rng.next_bool() {
                            // Expand
                            p.minimum -= 0.5;
                            p.maximum += 0.5;
                        } else {
                            // Shrink
                            p.minimum += 0.2;
                            p.maximum -= 0.2;
                        }
                        let res = doc.replace_parameter(p);
                        edit_result = Some(res.status);
                    }
                }
                6 => {
                    // Create Mesh
                    id_counter += 1;
                    let new_id = make_uuid(0x0006, id_counter);
                    let asset_id = rng.choose(doc.asset_order()).cloned().unwrap();
                    let part_id = rng.choose(doc.part_order()).cloned().unwrap();
                    let deformer_id = if rng.next_bool() && !doc.transform_order().is_empty() {
                        rng.choose(doc.transform_order()).cloned().unwrap()
                    } else {
                        String::new()
                    };
                    let res = doc.create_mesh(Mesh {
                        id: new_id,
                        runtime_id: format!("Mesh_{id_counter}"),
                        name: format!("Mesh {id_counter}"),
                        texture_asset_id: asset_id,
                        part_id,
                        deformer_id,
                        vertex_ids: vec![10, 20, 30],
                        base_positions: vec![
                            Vec2::new(0.0, 0.0),
                            Vec2::new(50.0, 0.0),
                            Vec2::new(25.0, 50.0),
                        ],
                        uvs: vec![
                            Vec2::new(0.0, 0.0),
                            Vec2::new(1.0, 0.0),
                            Vec2::new(0.5, 1.0),
                        ],
                        triangles: vec![[10, 20, 30]],
                        draw_order: Some(rng.next_f32(0.0, 10.0)),
                        ..Default::default()
                    });
                    edit_result = Some(res.status);
                }
                7 => {
                    // Reparent Mesh
                    if !doc.mesh_order().is_empty() {
                        let m_id = rng.choose(doc.mesh_order()).cloned().unwrap();
                        let mut m = doc.get_mesh(&m_id).unwrap().clone();
                        m.part_id = rng.choose(doc.part_order()).cloned().unwrap();
                        m.deformer_id = if rng.next_bool() && !doc.transform_order().is_empty() {
                            rng.choose(doc.transform_order()).cloned().unwrap()
                        } else {
                            String::new()
                        };
                        let res = doc.replace_mesh(m);
                        edit_result = Some(res.status);
                    }
                }
                8 => {
                    // Update Mesh Vertex Positions
                    if !doc.mesh_order().is_empty() && !doc.transaction_active() {
                        let m_id = rng.choose(doc.mesh_order()).cloned().unwrap();
                        let m = doc.get_mesh(&m_id).unwrap();
                        let updates: Vec<VertexPositionUpdate> = m
                            .vertex_ids
                            .iter()
                            .enumerate()
                            .map(|(idx, &vid)| VertexPositionUpdate {
                                mesh_id: m_id.clone(),
                                vertex_ids: vec![vid],
                                positions: vec![Vec2::new(
                                    m.base_positions[idx].x + rng.next_f32(-2.0, 2.0),
                                    m.base_positions[idx].y + rng.next_f32(-2.0, 2.0),
                                )],
                            })
                            .collect();
                        let res = doc.apply_vertex_position_updates(&updates);
                        edit_result = Some(res.status);
                    }
                }
                9 => {
                    // Modify Mesh Keyform
                    if !doc.binding_order().is_empty() {
                        let b_id = rng.choose(doc.binding_order()).cloned().unwrap();
                        let b = doc.get_binding(&b_id).unwrap();
                        let mut form = b.keyforms[0].clone();
                        for pt in &mut form.positions {
                            pt.x += rng.next_f32(-1.0, 1.0);
                        }
                        let res = doc.set_mesh_keyform(&b_id, form);
                        edit_result = Some(res.status);
                    }
                }
                10 => {
                    // Erase random object (could be referenced, testing reference safety)
                    let kind = rng.next_range(0, 4);
                    let to_erase = match kind {
                        0 if !doc.part_order().is_empty() => rng.choose(doc.part_order()).cloned(),
                        1 if !doc.transform_order().is_empty() => {
                            rng.choose(doc.transform_order()).cloned()
                        }
                        2 if !doc.mesh_order().is_empty() => rng.choose(doc.mesh_order()).cloned(),
                        3 if !doc.parameter_order().is_empty() => {
                            rng.choose(doc.parameter_order()).cloned()
                        }
                        _ => None,
                    };
                    if let Some(id) = to_erase {
                        let res = doc.erase_object(&id);
                        edit_result = Some(res.status);
                    }
                }
                11 => {
                    // Transaction: Begin
                    if !doc.transaction_active() {
                        tx_snapshot = Some(doc.clone());
                        let s = doc.begin_transaction();
                        edit_result = Some(s);
                    }
                }
                12 => {
                    // Transaction: Stage vertex position
                    if doc.transaction_active() && !doc.mesh_order().is_empty() {
                        let m_id = rng.choose(doc.mesh_order()).cloned().unwrap();
                        let m = doc.get_mesh(&m_id).unwrap();
                        let vid = m.vertex_ids[0];
                        let s = doc.stage_vertex_positions(VertexPositionUpdate {
                            mesh_id: m_id,
                            vertex_ids: vec![vid],
                            positions: vec![Vec2::new(
                                m.base_positions[0].x + rng.next_f32(-1.0, 1.0),
                                m.base_positions[0].y + rng.next_f32(-1.0, 1.0),
                            )],
                        });
                        edit_result = Some(s);
                    }
                }
                13 => {
                    // Transaction: Commit
                    if doc.transaction_active() {
                        let res = doc.commit_transaction();
                        tx_snapshot = None;
                        edit_result = Some(res.status);
                    }
                }
                14 => {
                    // Transaction: Cancel
                    if doc.transaction_active() {
                        let s = doc.cancel_transaction();
                        assert!(s.is_ok(), "cancel_transaction must succeed");
                        if let Some(snap) = tx_snapshot.take() {
                            assert!(
                                doc.same_content(&snap),
                                "cancel_transaction did not restore exact document content at {label}"
                            );
                        }
                        edit_result = Some(s);
                    }
                }
                15 => {
                    // Negative check: Attempt cyclic transform parent
                    if doc.transform_order().len() >= 2 {
                        let t0 = doc.transform_order()[0].clone();
                        let _t1 = doc.transform_order()[1].clone();
                        let mut t = doc.get_transform(&t0).unwrap().clone();
                        t.parent_id = t0.clone(); // self-cycle
                        let res = doc.replace_transform(t);
                        assert!(
                            !res.status.is_ok(),
                            "Self-cycle must be rejected at {label}"
                        );
                        edit_result = Some(res.status);
                    }
                }
                16 => {
                    // Negative check: Add invalid opacity
                    if !doc.scene_binding_order().is_empty() {
                        let sb_id = rng.choose(doc.scene_binding_order()).cloned().unwrap();
                        let sb = doc.get_scene_binding(&sb_id).unwrap();
                        let mut f = sb.keyforms[0].clone();
                        f.appearance.opacity = 2.5; // Invalid!
                        let res = doc.set_scene_keyform(&sb_id, f);
                        assert!(
                            !res.status.is_ok(),
                            "Invalid opacity 2.5 must be rejected at {label}"
                        );
                        edit_result = Some(res.status);
                    }
                }
                17 => {
                    // Mark saved checkpoint
                    doc.mark_saved();
                    assert!(
                        !doc.modified(),
                        "Document must not be modified after mark_saved"
                    );
                }
                _ => {}
            }

            // Failure Atomicity Invariant:
            if let Some(status) = edit_result {
                // These generated operations have valid IDs, references and values.
                let must_succeed = matches!(op_type, 0 | 2 | 4 | 6 | 7 | 8 | 9 | 11)
                    && !was_transaction_active
                    || matches!(op_type, 12..=14) && was_transaction_active;
                if must_succeed {
                    assert!(
                        status.is_ok(),
                        "Valid operation {op_type} rejected at {label}: {status:?}"
                    );
                    if matches!(op_type, 0 | 2 | 4 | 6 | 8 | 9) {
                        assert!(
                            !doc.same_content(&doc_before),
                            "Successful edit {op_type} had no effect at {label}"
                        );
                        assert!(
                            doc.revision() > rev_before,
                            "Successful edit must advance revision at {label}"
                        );
                    }
                }
                if !status.is_ok() {
                    assert_eq!(
                        doc.revision(),
                        rev_before,
                        "Failed operation must not advance revision at {label}"
                    );
                    assert_eq!(
                        doc.modified(),
                        mod_before,
                        "Failed operation must not change modified state at {label}"
                    );
                    assert!(
                        doc.same_content(&doc_before),
                        "Failed operation must not mutate document content at {label}"
                    );
                }
            }

            // Verify full invariants after every step (if outside of active transaction stage)
            if !doc.transaction_active() {
                verify_invariants(&doc, &label);
            }
        }
    }
}
