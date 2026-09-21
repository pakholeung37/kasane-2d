mod common;
use common::purism::*;

use std::collections::HashMap;
use std::os::raw::c_void;

use kasane_core::evaluation::{evaluate_frame, DrawableFrame, PreviewValues};
use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform,
    Parameter, Part, RotationPose, SceneBinding, SceneKeyform, Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use kasane_moc3::{encode_moc3, Moc3Artifact};

fn near(actual: f32, expected: f32, ppu: f32) {
    assert!(actual.is_finite(), "actual is not finite");
    assert!(expected.is_finite(), "expected is not finite");
    let e = (actual as f64 - expected as f64).abs();
    assert!(
        e <= 1e-4 + 1e-4 * (actual as f64).abs().max((expected as f64).abs())
            || e * ppu as f64 <= 0.05,
        "expected {}, actual {}, diff {}, pixel_diff {}",
        expected,
        actual,
        e,
        e * ppu as f64
    );
}

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn sid(n: i32) -> String {
    format!("{n:08x}-2222-4222-8222-222222222222")
}

fn fixture() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(271.0, 193.0).into(),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            name: "A".to_string(),
            source: "textures/0.png".to_string(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(3),
            name: "B".to_string(),
            source: "textures/1.png".to_string(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());

    let m1 = Mesh {
        id: id(4),
        name: "Asymmetric quad".to_string(),
        runtime_id: "ArtMeshA".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![91, 8, 77, 12],
        base_positions: vec![
            Vec2::new(101.0, 43.0),
            Vec2::new(328.0, 65.0),
            Vec2::new(365.0, 274.0),
            Vec2::new(87.0, 291.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.1),
            Vec2::new(0.9, 1.0),
            Vec2::new(0.0, 0.8),
        ],
        triangles: vec![[91, 8, 77], [91, 77, 12]],
        ..Default::default()
    };
    assert!(doc.create_mesh(m1).status.is_ok());

    let m2 = Mesh {
        id: id(5),
        name: "Triangle".to_string(),
        runtime_id: "ArtMeshB".to_string(),
        texture_asset_id: id(3),
        vertex_ids: vec![70, 13, 99],
        base_positions: vec![
            Vec2::new(350.0, 117.0),
            Vec2::new(542.0, 133.0),
            Vec2::new(468.0, 373.0),
        ],
        uvs: vec![
            Vec2::new(0.15, 0.05),
            Vec2::new(0.95, 0.2),
            Vec2::new(0.6, 0.9),
        ],
        triangles: vec![[70, 13, 99]],
        ..Default::default()
    };
    assert!(doc.create_mesh(m2).status.is_ok());
    doc
}

fn verify_with_purism(doc: &Document, artifact: &Moc3Artifact, preview: &PreviewValues) {
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(doc, preview, &mut frame).is_ok());
    let expected = &frame.drawables;

    unsafe {
        let mut moc_mem = AlignedBuffer::new(artifact.bytes.len(), 64);
        std::ptr::copy_nonoverlapping(
            artifact.bytes.as_ptr(),
            moc_mem.as_mut_ptr(),
            artifact.bytes.len(),
        );

        let consistent = csmHasMocConsistency(
            moc_mem.as_mut_ptr() as *mut c_void,
            artifact.bytes.len() as u32,
        );
        assert_eq!(consistent, 1, "csmHasMocConsistency failed");

        let moc = csmReviveMocInPlace(
            moc_mem.as_mut_ptr() as *mut c_void,
            artifact.bytes.len() as u32,
        );
        assert!(!moc.is_null(), "csmReviveMocInPlace returned null");

        let size = csmGetSizeofModel(moc);
        assert!(size > 0, "csmGetSizeofModel returned 0");

        let mut model_mem = AlignedBuffer::new(size as usize, 16);
        let model = csmInitializeModelInPlace(moc, model_mem.as_mut_ptr() as *mut c_void, size);
        assert!(!model.is_null(), "csmInitializeModelInPlace returned null");

        assert_eq!(
            csmGetParameterCount(model),
            doc.parameter_order().len() as i32
        );

        let param_ids = csmGetParameterIds(model);
        let param_mins = csmGetParameterMinimumValues(model);
        let param_maxs = csmGetParameterMaximumValues(model);
        let param_defs = csmGetParameterDefaultValues(model);
        let param_vals = csmGetParameterValues(model);

        for (i, pid) in doc.parameter_order().iter().enumerate() {
            let p = doc.get_parameter(pid).unwrap();
            let actual_id = c_str_to_str(*param_ids.add(i));
            assert_eq!(actual_id, p.runtime_id);
            near(*param_mins.add(i), p.minimum, 1.0);
            near(*param_maxs.add(i), p.maximum, 1.0);
            near(*param_defs.add(i), p.default_value, 1.0);

            *param_vals.add(i) = frame.parameters[i].value;
        }

        csmUpdateModel(model);

        let parts = doc.sorted_parts();
        assert_eq!(csmGetPartCount(model), parts.len() as i32);
        let part_ids = csmGetPartIds(model);
        let part_parents = csmGetPartParentPartIndices(model);

        for (i, pid) in parts.iter().enumerate() {
            let p = doc.get_part(pid).unwrap();
            let actual_id = c_str_to_str(*part_ids.add(i));
            assert_eq!(actual_id, p.runtime_id);
            let expected_parent = if p.parent_id.is_empty() {
                -1
            } else {
                parts.iter().position(|x| x == &p.parent_id).unwrap() as i32
            };
            assert_eq!(*part_parents.add(i), expected_parent);
        }

        assert_eq!(csmGetDrawableCount(model), expected.len() as i32);

        let mut canvas_size = CsmVector2 { x: 0.0, y: 0.0 };
        let mut canvas_origin = CsmVector2 { x: 0.0, y: 0.0 };
        let mut ppu: f32 = 0.0;
        csmReadCanvasInfo(model, &mut canvas_size, &mut canvas_origin, &mut ppu);

        near(canvas_size.x, doc.canvas().width, 1.0);
        near(canvas_size.y, doc.canvas().height, 1.0);
        near(canvas_origin.x, doc.canvas().origin.x, 1.0);
        near(
            canvas_origin.y,
            doc.canvas().height - doc.canvas().origin.y,
            1.0,
        );
        near(ppu, doc.canvas().pixels_per_unit, 1.0);

        let draw_ids = csmGetDrawableIds(model);
        let draw_tex = csmGetDrawableTextureIndices(model);
        let draw_vert_cnt = csmGetDrawableVertexCounts(model);
        let draw_idx_cnt = csmGetDrawableIndexCounts(model);
        let draw_orders = csmGetDrawableDrawOrders(model);
        let render_orders = csmGetRenderOrders(model);
        let draw_masks_cnt = csmGetDrawableMaskCounts(model);
        let draw_masks = csmGetDrawableMasks(model);
        let draw_parents = csmGetDrawableParentPartIndices(model);
        let draw_const_flags = csmGetDrawableConstantFlags(model);
        let draw_dyn_flags = csmGetDrawableDynamicFlags(model);
        let draw_opacities = csmGetDrawableOpacities(model);
        let draw_mul_colors = csmGetDrawableMultiplyColors(model);
        let draw_scr_colors = csmGetDrawableScreenColors(model);
        let draw_pos = csmGetDrawableVertexPositions(model);
        let draw_uvs = csmGetDrawableVertexUvs(model);
        let draw_indices = csmGetDrawableIndices(model);

        for (i, d) in expected.iter().enumerate() {
            let actual_id = c_str_to_str(*draw_ids.add(i));
            assert_eq!(actual_id, d.runtime_id);
            assert_eq!(*draw_tex.add(i), d.texture_slot);
            assert_eq!(*draw_vert_cnt.add(i), d.positions.len() as i32);
            assert_eq!(*draw_idx_cnt.add(i), d.indices.len() as i32);

            if d.enabled {
                assert_eq!(*draw_orders.add(i), d.draw_order);
            }
            assert_eq!(*render_orders.add(i), d.render_order);
            assert_eq!(*draw_masks_cnt.add(i), d.masks.len() as i32);

            let mask_slice = *draw_masks.add(i);
            for (k, mask_id) in d.masks.iter().enumerate() {
                let expected_mask_idx =
                    doc.mesh_order().iter().position(|x| x == mask_id).unwrap() as i32;
                assert_eq!(*mask_slice.add(k), expected_mask_idx);
            }

            let mesh_part = &doc.get_mesh(&d.id).unwrap().part_id;
            let expected_part_idx = if mesh_part.is_empty() {
                -1
            } else {
                parts.iter().position(|x| x == mesh_part).unwrap() as i32
            };
            assert_eq!(*draw_parents.add(i), expected_part_idx);

            let expected_flags: u8 = (if d.double_sided { 4 } else { 0 })
                | (if d.inverted_mask { 8 } else { 0 })
                | match d.blend_mode {
                    BlendMode::Additive => 1,
                    BlendMode::Multiplicative => 2,
                    BlendMode::Normal => 0,
                };
            assert_eq!(*draw_const_flags.add(i), expected_flags);
            assert_eq!((*draw_dyn_flags.add(i) & 1) != 0, d.visible);

            if d.enabled {
                near(*draw_opacities.add(i), d.opacity, 1.0);
            }

            let mul = *draw_mul_colors.add(i);
            let scr = *draw_scr_colors.add(i);
            if d.enabled {
                near(mul.x, d.multiply_color[0], 1.0);
                near(mul.y, d.multiply_color[1], 1.0);
                near(mul.z, d.multiply_color[2], 1.0);
                near(mul.w, 1.0, 1.0);

                near(scr.x, d.screen_color[0], 1.0);
                near(scr.y, d.screen_color[1], 1.0);
                near(scr.z, d.screen_color[2], 1.0);
                near(scr.w, 1.0, 1.0);
            }

            let pos_slice = *draw_pos.add(i);
            let uv_slice = *draw_uvs.add(i);
            for j in 0..d.positions.len() {
                let p = *pos_slice.add(j);
                let uv = *uv_slice.add(j);
                if d.visible {
                    near(p.x, d.positions[j].x, ppu);
                    near(p.y, d.positions[j].y, ppu);
                }
                near(uv.x, d.uvs[j].x, 1.0);
                near(uv.y, d.uvs[j].y, 1.0);
            }

            let idx_slice = *draw_indices.add(i);
            for j in 0..d.indices.len() {
                assert_eq!(*idx_slice.add(j) as u32, d.indices[j]);
            }
        }
    }
}

#[test]
fn test_fixture_export_and_purism_verification() {
    let mut doc = fixture();
    let initial = encode_moc3(&doc).expect("encode_moc3 failed");
    assert_eq!(initial.bytes[0..5], *b"MOC3\x05");
    assert_eq!(initial.textures.len(), 2);

    verify_with_purism(&doc, &initial, &HashMap::new());

    // Rename mesh: display name change should NOT alter binary MOC3 bytes
    assert!(doc
        .rename_mesh(&id(4), "renamed".to_string())
        .status
        .is_ok());
    let same = encode_moc3(&doc).unwrap();
    assert_eq!(same.bytes, initial.bytes);

    // Edit vertex positions: should alter bytes and still verify with PurismCore
    let p = Vec2::new(338.0, 69.0);
    assert!(doc.set_vertex_positions(&id(4), &[8], &[p]).status.is_ok());
    let edited = encode_moc3(&doc).unwrap();
    assert_ne!(edited.bytes, initial.bytes);
    verify_with_purism(&doc, &edited, &HashMap::new());
}

#[test]
fn test_reject_malformed() {
    let doc = fixture();
    let artifact = encode_moc3(&doc).unwrap();

    unsafe {
        let mut memory = AlignedBuffer::new(artifact.bytes.len(), 64);
        std::ptr::copy_nonoverlapping(
            artifact.bytes.as_ptr(),
            memory.as_mut_ptr(),
            artifact.bytes.len(),
        );

        // Truncated buffer
        assert_eq!(
            csmHasMocConsistency(memory.as_mut_ptr() as *mut c_void, 64),
            0
        );

        // Corrupt version byte
        *memory.as_mut_ptr().add(4) = 255;
        assert_eq!(
            csmHasMocConsistency(
                memory.as_mut_ptr() as *mut c_void,
                artifact.bytes.len() as u32
            ),
            0
        );
    }
}

#[test]
fn test_animated_keyforms_1d_2d_3d() {
    for dimensions in 1..=3 {
        let mut doc = fixture();
        let mut b = MeshBinding {
            id: id(9),
            mesh_id: id(4),
            axes: Vec::new(),
            keyforms: Vec::new(),
        };

        for a in 0..dimensions {
            let pid = id(6 + a);
            assert!(doc
                .create_parameter(Parameter {
                    id: pid.clone(),
                    runtime_id: format!("Param{a}"),
                    name: "param".to_string(),
                    minimum: -1.0,
                    maximum: 1.0,
                    default_value: 0.0,
                    decimal_places: 6,
                    ..Default::default()
                })
                .status
                .is_ok());
        }

        let keys: Vec<f32> = if dimensions == 3 {
            vec![-1.0, 1.0]
        } else {
            vec![-1.0, 0.0, 1.0]
        };

        for a in 0..dimensions {
            b.axes.push(BindingAxis {
                parameter_id: id(6 + a),
                keys: keys.clone(),
            });
        }

        let total: usize = keys.len().pow(dimensions as u32);
        for i in 0..total {
            let mut f = MeshKeyform {
                keys: Vec::new(),
                positions: doc.get_mesh(&id(4)).unwrap().base_positions.clone(),
                appearance: Appearance::default(),
                draw_order: None,
            };
            let mut cursor = i;
            for _ in 0..dimensions {
                f.keys.push(keys[cursor % keys.len()]);
                cursor /= keys.len();
            }
            let x = f.keys[0];
            let y = if dimensions > 1 { f.keys[1] } else { 0.0 };
            let z = if dimensions > 2 { f.keys[2] } else { 0.0 };
            for (v, pt) in f.positions.iter_mut().enumerate() {
                pt.x += 11.0 * x + 3.0 * y + 7.0 * z + (v as f32) * x * y * 2.0;
                pt.y += 5.0 * x - 13.0 * y + 2.0 * z + (v as f32) * x * z * 3.0;
            }
            b.keyforms.push(f);
        }
        b.keyforms.reverse();
        assert!(doc.create_binding(b).status.is_ok());

        let artifact = encode_moc3(&doc).unwrap();
        let values: Vec<f32> = if dimensions == 3 {
            vec![-1.0, 0.0, 1.0]
        } else {
            vec![-1.0, -0.5, 0.0, 0.5, 1.0]
        };

        let total_samples = values.len().pow(dimensions as u32);
        for i in 0..total_samples {
            let mut preview = HashMap::new();
            let mut cursor = i;
            for a in 0..dimensions {
                preview.insert(id(6 + a), values[cursor % values.len()]);
                cursor /= values.len();
            }
            verify_with_purism(&doc, &artifact, &preview);
        }
    }
}

fn scene_fixture(warp_root: bool, quad: bool) -> Document {
    let mut doc = fixture();
    assert!(doc
        .create_parameter(Parameter {
            id: id(6),
            runtime_id: "ParamScene".to_string(),
            name: "scene".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .create_part(Part {
            id: sid(1),
            runtime_id: "PartRoot".to_string(),
            name: "root".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 12.0,
        })
        .status
        .is_ok());

    assert!(doc
        .create_part(Part {
            id: sid(2),
            runtime_id: "PartChild".to_string(),
            name: "child".to_string(),
            parent_id: sid(1),
            enabled: true,
            draw_order: -3.0,
        })
        .status
        .is_ok());

    let root = Transform {
        id: sid(3),
        runtime_id: "RootTransform".to_string(),
        name: "root_tfm".to_string(),
        part_id: sid(1),
        kind: if warp_root {
            TransformKind::Warp
        } else {
            TransformKind::Rotation
        },
        rotation: RotationPose {
            origin: Vec2::new(312.0, 207.0).into(),
            angle: 13.0,
            scale: 1.17,
            reflect_x: true,
            reflect_y: false,
        },
        base_angle: 7.0,
        rows: 2,
        columns: 2,
        quad,
        points: vec![
            Vec2::new(100.0, 390.0),
            Vec2::new(290.0, 380.0),
            Vec2::new(500.0, 370.0),
            Vec2::new(90.0, 215.0),
            Vec2::new(300.0, 200.0),
            Vec2::new(520.0, 195.0),
            Vec2::new(70.0, 40.0),
            Vec2::new(305.0, 35.0),
            Vec2::new(530.0, 20.0),
        ],
        appearance: Appearance {
            opacity: 0.83,
            multiply: [0.9, 0.8, 0.95],
            screen: [0.1, 0.2, 0.05],
        },
        ..Default::default()
    };
    assert!(doc.create_transform(root.clone()).status.is_ok());

    let child = Transform {
        id: sid(4),
        runtime_id: "ChildTransform".to_string(),
        name: "child_tfm".to_string(),
        parent_id: root.id.clone(),
        part_id: sid(2),
        kind: if warp_root {
            TransformKind::Rotation
        } else {
            TransformKind::Warp
        },
        rotation: RotationPose {
            origin: Vec2::new(0.37, 0.63).into(),
            angle: -24.0,
            scale: 0.86,
            reflect_x: false,
            reflect_y: true,
        },
        base_angle: -9.0,
        rows: 2,
        columns: 2,
        quad,
        points: vec![
            Vec2::new(-1.0, -1.0),
            Vec2::new(0.1, -1.1),
            Vec2::new(1.2, -1.0),
            Vec2::new(-1.1, 0.0),
            Vec2::new(0.15, 0.2),
            Vec2::new(1.3, 0.1),
            Vec2::new(-0.9, 1.2),
            Vec2::new(0.0, 1.1),
            Vec2::new(1.1, 1.4),
        ],
        appearance: Appearance {
            opacity: 0.77,
            multiply: [0.8, 1.0, 0.9],
            screen: [0.05, 0.1, 0.2],
        },
        ..Default::default()
    };
    assert!(doc.create_transform(child.clone()).status.is_ok());

    for t in [&root, &child] {
        let mut b = SceneBinding {
            id: sid(if t.id == root.id { 5 } else { 6 }),
            target_id: t.id.clone(),
            axes: vec![BindingAxis {
                parameter_id: id(6),
                keys: vec![-1.0, 0.0, 1.0],
            }],
            keyforms: Vec::new(),
        };
        for &key in &[-1.0f32, 0.0, 1.0] {
            let mut f = SceneKeyform {
                keys: vec![key],
                rotation: t.rotation,
                positions: Vec::new(),
                appearance: t.appearance,
                draw_order: 0.0,
            };
            f.rotation.angle += key * 17.0;
            f.rotation.scale += key * 0.13;
            f.rotation.origin.x += key as f64 * if t.parent_id.is_empty() { 11.0 } else { 0.07 };
            f.appearance.opacity += key * 0.05;
            if t.kind == TransformKind::Warp {
                f.positions = t.points.clone();
                for (i, p) in f.positions.iter_mut().enumerate() {
                    p.x += key * (i % 3) as f32 * if t.parent_id.is_empty() { 7.0 } else { 0.09 };
                    p.y += key * (i / 3) as f32 * if t.parent_id.is_empty() { 3.0 } else { 0.03 };
                }
            }
            b.keyforms.push(f);
        }
        assert!(doc.create_scene_binding(b).status.is_ok());
    }

    let mut mesh = doc.get_mesh(&id(4)).unwrap().clone();
    mesh.part_id = sid(2);
    mesh.deformer_id = child.id.clone();
    mesh.base_positions = vec![
        Vec2::new(-0.25, 0.15),
        Vec2::new(1.23, 0.31),
        Vec2::new(3.1, 1.2),
        Vec2::new(-2.2, 0.83),
    ];
    mesh.draw_order = Some(18.0);
    mesh.appearance = Appearance {
        opacity: 0.71,
        multiply: [0.7, 0.85, 0.9],
        screen: [0.1, 0.05, 0.17],
    };
    mesh.masks = vec![id(5)];
    mesh.inverted_mask = warp_root;
    mesh.blend_mode = if warp_root {
        BlendMode::Multiplicative
    } else {
        BlendMode::Additive
    };
    assert!(doc.replace_mesh(mesh).status.is_ok());

    let mut other = doc.get_mesh(&id(5)).unwrap().clone();
    other.draw_order = Some(25.0);
    assert!(doc.replace_mesh(other).status.is_ok());

    let mut part_binding = SceneBinding {
        id: sid(7),
        target_id: sid(1),
        axes: vec![BindingAxis {
            parameter_id: id(6),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: Vec::new(),
    };
    for &key in &[-1.0f32, 0.0, 1.0] {
        part_binding.keyforms.push(SceneKeyform {
            keys: vec![key],
            draw_order: key * 20.0 + 20.0,
            ..Default::default()
        });
    }
    assert!(doc.create_scene_binding(part_binding).status.is_ok());

    doc
}

#[test]
fn test_nested_scene_hierarchy() {
    for warp_root in [false, true] {
        for quad in [false, true] {
            let doc = scene_fixture(warp_root, quad);
            let artifact = encode_moc3(&doc).unwrap();

            for p in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
                let mut preview = HashMap::new();
                preview.insert(id(6), p);
                verify_with_purism(&doc, &artifact, &preview);
            }
        }
    }
}

#[test]
fn test_error_handling_and_validations() {
    let mut doc = fixture();
    assert!(doc.begin_transaction().is_ok());
    let err = encode_moc3(&doc).unwrap_err();
    assert_eq!(err.code, "TRANSACTION_ACTIVE");
    assert!(doc.cancel_transaction().is_ok());

    // Unrepresentable ID (> 63 characters)
    let mut bad_mesh = doc.get_mesh(&id(4)).unwrap().clone();
    bad_mesh.runtime_id = "a".repeat(64);
    assert!(doc.replace_mesh(bad_mesh).status.is_ok());
    let err = encode_moc3(&doc).unwrap_err();
    assert_eq!(err.code, "UNREPRESENTABLE_ID");
}
