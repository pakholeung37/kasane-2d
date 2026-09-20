use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform,
    Parameter, Part, RotationPose, SceneBinding, SceneKeyform, Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use kasane_moc3::encode_moc3;
use serde::Serialize;

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn sid(n: i32) -> String {
    format!("{n:08x}-2222-4222-8222-222222222222")
}

#[derive(Serialize)]
struct ConformanceSample {
    parameters: Vec<f32>,
    drawables: Vec<ExpectedDrawable>,
}

#[derive(Serialize)]
struct ExpectedDrawable {
    id: String,
    runtime_id: String,
    texture_slot: i32,
    draw_order: f32,
    render_order: i32,
    double_sided: bool,
    inverted_mask: bool,
    blend_mode: i32,
    masks: Vec<String>,
    positions: Vec<Vec2>,
    uvs: Vec<Vec2>,
    indices: Vec<u32>,
    opacity: f32,
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
}

fn base_doc() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(271.0, 193.0),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            name: "Tex0".to_string(),
            source: "textures/0.png".to_string(),
            width: 8,
            height: 8,
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        })
        .status
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(3),
            name: "Tex1".to_string(),
            source: "textures/1.png".to_string(),
            width: 8,
            height: 8,
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        })
        .status
        .is_ok());

    doc
}

fn build_1d_case() -> (Document, Vec<Vec<f32>>) {
    let mut doc = base_doc();
    let p_id = id(10);
    assert!(doc
        .create_parameter(Parameter {
            id: p_id.clone(),
            runtime_id: "Param_X".to_string(),
            name: "X".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 4,
        })
        .status
        .is_ok());

    let m_id = id(20);
    assert!(doc
        .create_mesh(Mesh {
            id: m_id.clone(),
            runtime_id: "ArtMesh1".to_string(),
            name: "Mesh 1".to_string(),
            texture_asset_id: id(2),
            vertex_ids: vec![1, 2, 3, 4],
            base_positions: vec![
                Vec2::new(-30.0, 30.0),
                Vec2::new(-30.0, -30.0),
                Vec2::new(30.0, -30.0),
                Vec2::new(30.0, 30.0),
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
        })
        .status
        .is_ok());

    assert!(doc
        .create_binding(MeshBinding {
            id: id(30),
            mesh_id: m_id,
            axes: vec![BindingAxis {
                parameter_id: p_id,
                keys: vec![-1.0, 0.0, 1.0],
            }],
            keyforms: vec![
                MeshKeyform {
                    keys: vec![-1.0],
                    positions: vec![
                        Vec2::new(-40.0, 30.0),
                        Vec2::new(-40.0, -30.0),
                        Vec2::new(20.0, -30.0),
                        Vec2::new(20.0, 30.0),
                    ],
                    appearance: Appearance {
                        opacity: 0.5,
                        ..Default::default()
                    },
                    draw_order: None,
                },
                MeshKeyform {
                    keys: vec![0.0],
                    positions: vec![
                        Vec2::new(-30.0, 30.0),
                        Vec2::new(-30.0, -30.0),
                        Vec2::new(30.0, -30.0),
                        Vec2::new(30.0, 30.0),
                    ],
                    appearance: Appearance::default(),
                    draw_order: None,
                },
                MeshKeyform {
                    keys: vec![1.0],
                    positions: vec![
                        Vec2::new(-20.0, 30.0),
                        Vec2::new(-20.0, -30.0),
                        Vec2::new(40.0, -30.0),
                        Vec2::new(40.0, 30.0),
                    ],
                    appearance: Appearance {
                        opacity: 0.9,
                        ..Default::default()
                    },
                    draw_order: None,
                },
            ],
        })
        .status
        .is_ok());

    let samples = vec![vec![-1.0], vec![-0.5], vec![0.0], vec![0.5], vec![1.0]];
    (doc, samples)
}

fn build_2d_case() -> (Document, Vec<Vec<f32>>) {
    let mut doc = base_doc();
    let p_x = id(11);
    let p_y = id(12);
    for (pid, rid) in [(&p_x, "ParamX"), (&p_y, "ParamY")] {
        assert!(doc
            .create_parameter(Parameter {
                id: pid.clone(),
                runtime_id: rid.to_string(),
                name: rid.to_string(),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                decimal_places: 4,
            })
            .status
            .is_ok());
    }

    let m_id = id(21);
    assert!(doc
        .create_mesh(Mesh {
            id: m_id.clone(),
            runtime_id: "ArtMesh2D".to_string(),
            name: "Mesh 2D".to_string(),
            texture_asset_id: id(3),
            vertex_ids: vec![10, 20, 30],
            base_positions: vec![
                Vec2::new(0.0, 40.0),
                Vec2::new(-30.0, -20.0),
                Vec2::new(30.0, -20.0),
            ],
            uvs: vec![
                Vec2::new(0.5, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
            triangles: vec![[10, 20, 30]],
            draw_order: Some(2.0),
            ..Default::default()
        })
        .status
        .is_ok());

    let mut b = MeshBinding {
        id: id(31),
        mesh_id: m_id,
        axes: vec![
            BindingAxis {
                parameter_id: p_x,
                keys: vec![-1.0, 1.0],
            },
            BindingAxis {
                parameter_id: p_y,
                keys: vec![-1.0, 1.0],
            },
        ],
        keyforms: Vec::new(),
    };

    for &y in &[-1.0f32, 1.0] {
        for &x in &[-1.0f32, 1.0] {
            b.keyforms.push(MeshKeyform {
                keys: vec![x, y],
                positions: vec![
                    Vec2::new(x * 10.0, 40.0 + y * 10.0),
                    Vec2::new(-30.0 + x * 10.0, -20.0 + y * 10.0),
                    Vec2::new(30.0 + x * 10.0, -20.0 + y * 10.0),
                ],
                appearance: Appearance::default(),
                draw_order: None,
            });
        }
    }
    assert!(doc.create_binding(b).status.is_ok());

    let mut samples = Vec::new();
    for &x in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
        for &y in &[-1.0, 0.0, 1.0] {
            samples.push(vec![x, y]);
        }
    }
    (doc, samples)
}

fn build_masks_and_blends_case() -> (Document, Vec<Vec<f32>>) {
    let mut doc = base_doc();
    let p_id = id(15);
    assert!(doc
        .create_parameter(Parameter {
            id: p_id.clone(),
            runtime_id: "Param_Mask".to_string(),
            name: "Mask Param".to_string(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 4,
        })
        .status
        .is_ok());

    let mask_mesh_id = id(22);
    assert!(doc
        .create_mesh(Mesh {
            id: mask_mesh_id.clone(),
            runtime_id: "MaskMesh".to_string(),
            name: "Mask Mesh".to_string(),
            texture_asset_id: id(2),
            vertex_ids: vec![1, 2, 3],
            base_positions: vec![
                Vec2::new(-50.0, 50.0),
                Vec2::new(-50.0, -50.0),
                Vec2::new(50.0, -50.0),
            ],
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
            triangles: vec![[1, 2, 3]],
            draw_order: Some(0.0),
            ..Default::default()
        })
        .status
        .is_ok());

    // Clipped mesh with Additive blend
    let clipped_mesh_id = id(23);
    assert!(doc
        .create_mesh(Mesh {
            id: clipped_mesh_id,
            runtime_id: "ClippedAdd".to_string(),
            name: "Clipped Additive Mesh".to_string(),
            texture_asset_id: id(3),
            vertex_ids: vec![11, 12, 13],
            base_positions: vec![
                Vec2::new(-40.0, 40.0),
                Vec2::new(-40.0, -40.0),
                Vec2::new(40.0, -40.0),
            ],
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
            triangles: vec![[11, 12, 13]],
            masks: vec![mask_mesh_id.clone()],
            inverted_mask: false,
            blend_mode: BlendMode::Additive,
            draw_order: Some(1.0),
            ..Default::default()
        })
        .status
        .is_ok());

    // Inverted mask mesh with Multiply blend
    let inv_clipped_id = id(24);
    assert!(doc
        .create_mesh(Mesh {
            id: inv_clipped_id,
            runtime_id: "InvertedMul".to_string(),
            name: "Inverted Multiply Mesh".to_string(),
            texture_asset_id: id(2),
            vertex_ids: vec![21, 22, 23],
            base_positions: vec![
                Vec2::new(-30.0, 30.0),
                Vec2::new(-30.0, -30.0),
                Vec2::new(30.0, -30.0),
            ],
            uvs: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
            triangles: vec![[21, 22, 23]],
            masks: vec![mask_mesh_id],
            inverted_mask: true,
            blend_mode: BlendMode::Multiplicative,
            draw_order: Some(2.0),
            ..Default::default()
        })
        .status
        .is_ok());

    let samples = vec![vec![0.0], vec![0.5], vec![1.0]];
    (doc, samples)
}

fn build_nested_deformers_case() -> (Document, Vec<Vec<f32>>) {
    let mut doc = base_doc();
    let p_id = id(16);
    assert!(doc
        .create_parameter(Parameter {
            id: p_id.clone(),
            runtime_id: "Param_Rot".to_string(),
            name: "Rotation Param".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 4,
        })
        .status
        .is_ok());

    let part_id = id(40);
    assert!(doc
        .create_part(Part {
            id: part_id.clone(),
            runtime_id: "Part_Nested".to_string(),
            name: "Nested Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 1.0,
        })
        .status
        .is_ok());

    let rot_id = id(50);
    let rot = Transform {
        id: rot_id.clone(),
        runtime_id: "Rot_Parent".to_string(),
        name: "Rot Parent".to_string(),
        part_id: part_id.clone(),
        parent_id: String::new(),
        kind: TransformKind::Rotation,
        rotation: RotationPose {
            origin: Vec2::new(271.0, 193.0),
            angle: 0.0,
            scale: 1.0,
            reflect_x: false,
            reflect_y: false,
        },
        ..Default::default()
    };
    assert!(doc.create_transform(rot).status.is_ok());

    let warp_id = id(51);
    let mut warp = Transform {
        id: warp_id.clone(),
        runtime_id: "Warp_Child".to_string(),
        name: "Warp Child".to_string(),
        part_id: part_id.clone(),
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
                .push(Vec2::new(c as f32 * 40.0 - 40.0, r as f32 * 40.0 - 40.0));
        }
    }
    assert!(doc.create_transform(warp).status.is_ok());

    let m_id = id(25);
    assert!(doc
        .create_mesh(Mesh {
            id: m_id,
            runtime_id: "Mesh_Deformed".to_string(),
            name: "Deformed Mesh".to_string(),
            texture_asset_id: id(2),
            part_id,
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
            draw_order: Some(5.0),
            ..Default::default()
        })
        .status
        .is_ok());

    // Scene binding on the rotation transform
    assert!(doc
        .create_scene_binding(SceneBinding {
            id: sid(60),
            target_id: rot_id,
            axes: vec![BindingAxis {
                parameter_id: p_id,
                keys: vec![-1.0, 0.0, 1.0],
            }],
            keyforms: vec![
                SceneKeyform {
                    keys: vec![-1.0],
                    rotation: RotationPose {
                        origin: Vec2::new(271.0, 193.0),
                        angle: -30.0,
                        scale: 0.8,
                        reflect_x: false,
                        reflect_y: false,
                    },
                    ..Default::default()
                },
                SceneKeyform {
                    keys: vec![0.0],
                    rotation: RotationPose {
                        origin: Vec2::new(271.0, 193.0),
                        angle: 0.0,
                        scale: 1.0,
                        reflect_x: false,
                        reflect_y: false,
                    },
                    ..Default::default()
                },
                SceneKeyform {
                    keys: vec![1.0],
                    rotation: RotationPose {
                        origin: Vec2::new(271.0, 193.0),
                        angle: 30.0,
                        scale: 1.2,
                        reflect_x: false,
                        reflect_y: false,
                    },
                    ..Default::default()
                },
            ],
        })
        .status
        .is_ok());

    let samples = vec![vec![-1.0], vec![-0.6], vec![0.0], vec![0.6], vec![1.0]];
    (doc, samples)
}

fn export_case(
    out_dir: &std::path::Path,
    case_name: &str,
    doc: Document,
    param_samples: Vec<Vec<f32>>,
) {
    let case_path = out_dir.join(case_name);
    fs::create_dir_all(&case_path).expect("create case dir");

    let artifact = encode_moc3(&doc).expect("encode_moc3 failed");
    fs::write(case_path.join("model.moc3"), &artifact.bytes).expect("write model.moc3");
    fs::write(case_path.join("model.model3.json"), &artifact.model3_json)
        .expect("write model3.json");

    let mut samples = Vec::new();
    for sample in param_samples {
        let mut preview_map = HashMap::new();
        for (i, p_id) in doc.parameter_order().iter().enumerate() {
            preview_map.insert(p_id.clone(), sample[i]);
        }
        let mut frame = DrawableFrame::default();
        assert!(evaluate_frame(&doc, &preview_map, &mut frame).is_ok());

        let mut drawables = Vec::new();
        for d in frame.drawables {
            drawables.push(ExpectedDrawable {
                id: d.id,
                runtime_id: d.runtime_id,
                texture_slot: d.texture_slot,
                draw_order: d.draw_order as f32,
                render_order: d.render_order,
                double_sided: d.double_sided,
                inverted_mask: d.inverted_mask,
                blend_mode: match d.blend_mode {
                    BlendMode::Normal => 0,
                    BlendMode::Additive => 1,
                    BlendMode::Multiplicative => 2,
                },
                masks: d.masks,
                positions: d.positions,
                uvs: d.uvs,
                indices: d.indices,
                opacity: d.opacity,
                multiply_color: d.multiply_color,
                screen_color: d.screen_color,
            });
        }
        samples.push(ConformanceSample {
            parameters: sample,
            drawables,
        });
    }

    let samples_json = serde_json::to_string_pretty(&samples).expect("serialize samples");
    fs::write(case_path.join("samples.json"), samples_json).expect("write samples.json");
    println!("Exported conformance case: {}", case_name);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("target/kasane/official-conformance/fixtures")
    };

    println!("Exporting conformance cases to: {:?}", out_dir);
    fs::create_dir_all(&out_dir).expect("create out_dir");

    let (doc_1d, samples_1d) = build_1d_case();
    export_case(&out_dir, "case_1d", doc_1d, samples_1d);

    let (doc_2d, samples_2d) = build_2d_case();
    export_case(&out_dir, "case_2d", doc_2d, samples_2d);

    let (doc_masks, samples_masks) = build_masks_and_blends_case();
    export_case(&out_dir, "case_masks", doc_masks, samples_masks);

    let (doc_nested, samples_nested) = build_nested_deformers_case();
    export_case(&out_dir, "case_nested", doc_nested, samples_nested);

    println!("All conformance cases exported successfully.");
}
