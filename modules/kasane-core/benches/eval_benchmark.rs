use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use kasane_core::{
    BindingAxis, Canvas, Document, DrawableFrame, FrameEvaluator, ImageAsset, Mesh, MeshBinding,
    MeshKeyform, Parameter, Part, RotationPose, SceneBinding, SceneKeyform, Transform,
    TransformKind, Vec2, VertexId,
};

fn make_id(kind: u32, n: u32) -> String {
    format!("{:04x}{:04x}-1111-4111-8111-111111111111", kind, n)
}

fn build_benchmark_document(out_params: &mut Vec<String>) -> Document {
    let mut doc = Document::new();
    let doc_id = make_id(0x0001, 1);
    assert!(doc
        .initialize(
            doc_id,
            Canvas::new(1280.0, 720.0, Vec2::new(640.0, 360.0), 100.0)
        )
        .is_ok());

    let asset1 = make_id(0x0002, 1);
    let asset2 = make_id(0x0002, 2);
    assert!(doc
        .add_asset(ImageAsset {
            id: asset1.clone(),
            name: "texture_0".to_string(),
            source: "textures/0.png".to_string(),
            width: 512,
            height: 512,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            id: asset2.clone(),
            name: "texture_1".to_string(),
            source: "textures/1.png".to_string(),
            width: 512,
            height: 512,
            ..Default::default()
        })
        .status
        .is_ok());

    // 4 Parameters
    out_params.clear();
    for i in 0..4 {
        let pid = make_id(0x0003, i + 1);
        out_params.push(pid.clone());
        assert!(doc
            .create_parameter(Parameter {
                id: pid.clone(),
                runtime_id: format!("Param_{}", i),
                name: format!("Parameter {}", i),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                decimal_places: 6,
                ..Default::default()
            })
            .status
            .is_ok());
    }

    // 2 Parts
    let part_root = make_id(0x0004, 1);
    let part_child = make_id(0x0004, 2);
    assert!(doc
        .create_part(Part {
            id: part_root.clone(),
            runtime_id: "PartRoot".to_string(),
            name: "Root Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 0.0,
        })
        .status
        .is_ok());
    assert!(doc
        .create_part(Part {
            id: part_child.clone(),
            runtime_id: "PartChild".to_string(),
            name: "Child Part".to_string(),
            parent_id: part_root,
            enabled: true,
            draw_order: 1.0,
        })
        .status
        .is_ok());

    // Transform 1: Root Rotation
    let rot_root = make_id(0x0005, 1);
    let tfm_rot = Transform {
        id: rot_root.clone(),
        runtime_id: "RootRotation".to_string(),
        name: "Root Rotation".to_string(),
        part_id: part_child.clone(),
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
    };
    assert!(doc.create_transform(tfm_rot.clone()).status.is_ok());

    // Transform 2: Child Warp (3x3 grid = 16 points)
    let warp_child = make_id(0x0005, 2);
    let mut tfm_warp = Transform {
        id: warp_child.clone(),
        runtime_id: "ChildWarp".to_string(),
        name: "Child Warp".to_string(),
        part_id: part_child.clone(),
        parent_id: rot_root.clone(),
        kind: TransformKind::Warp,
        rows: 3,
        columns: 3,
        quad: true,
        points: Vec::new(),
        ..Default::default()
    };
    for r in 0..=3 {
        for c in 0..=3 {
            tfm_warp
                .points
                .push(Vec2::new(c as f32 * 50.0 - 75.0, r as f32 * 50.0 - 75.0));
        }
    }
    assert!(doc.create_transform(tfm_warp.clone()).status.is_ok());

    // Bindings for transforms
    for t in 1..=2 {
        let tid = if t == 1 {
            rot_root.clone()
        } else {
            warp_child.clone()
        };
        let mut sb = SceneBinding {
            id: make_id(0x0006, t),
            target_id: tid,
            axes: vec![BindingAxis {
                parameter_id: out_params[0].clone(),
                keys: vec![-1.0, 0.0, 1.0],
            }],
            keyforms: Vec::new(),
        };
        for key in [-1.0f32, 0.0, 1.0] {
            let mut kf = SceneKeyform {
                keys: vec![key],
                ..Default::default()
            };
            if t == 1 {
                kf.rotation = tfm_rot.rotation;
                kf.rotation.angle = key * 25.0;
                kf.rotation.scale = 1.0 + key * 0.1;
            } else {
                kf.positions = tfm_warp.points.clone();
                for pt in &mut kf.positions {
                    pt.x += key * 15.0;
                }
            }
            sb.keyforms.push(kf);
        }
        assert!(doc.create_scene_binding(sb).status.is_ok());
    }

    // 10 Meshes (each with 4x4 vertex grid = 16 vertices, 18 triangles)
    for m in 0..10 {
        let mesh_id = make_id(0x0007, m + 1);
        let mut mesh = Mesh {
            id: mesh_id.clone(),
            runtime_id: format!("Mesh_{}", m),
            name: format!("Mesh {}", m),
            texture_asset_id: if m % 2 == 0 {
                asset1.clone()
            } else {
                asset2.clone()
            },
            part_id: part_child.clone(),
            deformer_id: warp_child.clone(),
            draw_order: Some(m as f32),
            ..Default::default()
        };

        let mut vid: VertexId = 0;
        for r in 0..4 {
            for c in 0..4 {
                mesh.vertex_ids.push(vid);
                vid += 1;
                mesh.base_positions
                    .push(Vec2::new(c as f32 * 20.0 - 30.0, r as f32 * 20.0 - 30.0));
                mesh.uvs.push(Vec2::new(c as f32 / 3.0, r as f32 / 3.0));
            }
        }
        for r in 0..3 {
            for c in 0..3 {
                let v0 = (r * 4 + c) as VertexId;
                let v1 = (r * 4 + c + 1) as VertexId;
                let v2 = ((r + 1) * 4 + c) as VertexId;
                let v3 = ((r + 1) * 4 + c + 1) as VertexId;
                mesh.triangles.push([v0, v1, v2]);
                mesh.triangles.push([v1, v3, v2]);
            }
        }

        assert!(doc.create_mesh(mesh.clone()).status.is_ok());

        let mut mb = MeshBinding {
            id: make_id(0x0008, m + 1),
            mesh_id: mesh_id.clone(),
            axes: vec![BindingAxis {
                parameter_id: out_params[1].clone(),
                keys: vec![-1.0, 0.0, 1.0],
            }],
            keyforms: Vec::new(),
        };
        for key in [-1.0f32, 0.0, 1.0] {
            let mut mkf = MeshKeyform {
                keys: vec![key],
                positions: mesh.base_positions.clone(),
                ..Default::default()
            };
            for (vi, p) in mkf.positions.iter_mut().enumerate() {
                p.x += key * 10.0 * ((vi % 4) as f32 / 3.0);
                p.y += key * 5.0;
            }
            mb.keyforms.push(mkf);
        }
        assert!(doc.create_binding(mb).status.is_ok());
    }

    doc
}

fn main() {
    println!("=== Kasane 2D Rust Evaluation Benchmark ===");
    let mut param_ids = Vec::new();
    let doc = build_benchmark_document(&mut param_ids);

    let total_vertices: usize = doc
        .mesh_order()
        .iter()
        .map(|id| doc.get_mesh(id).unwrap().base_positions.len())
        .sum();
    println!(
        "Document: {} meshes, {} transforms, {} total vertices",
        doc.mesh_order().len(),
        doc.transform_order().len(),
        total_vertices
    );

    let mut frame = DrawableFrame::default();
    let mut evaluator = FrameEvaluator::default();
    let mut preview = HashMap::new();

    // Warmup
    for i in 0..100 {
        let val = (i as f32 * 0.1).sin();
        preview.insert(param_ids[0].clone(), val);
        preview.insert(param_ids[1].clone(), -val);
        assert!(evaluator.evaluate(&doc, &preview, &mut frame).is_ok());
    }

    let iterations = 5000;
    let start = Instant::now();
    for i in 0..iterations {
        let val = (i as f32 * 0.05).sin();
        preview.insert(param_ids[0].clone(), val);
        preview.insert(param_ids[1].clone(), -val * 0.8);
        assert!(evaluator
            .evaluate(black_box(&doc), black_box(&preview), &mut frame)
            .is_ok());
        black_box(&frame);
    }
    let elapsed = start.elapsed();
    let total_ms = elapsed.as_secs_f64() * 1000.0;
    let mean_us = (total_ms * 1000.0) / (iterations as f64);
    let fps = (iterations as f64 / total_ms) * 1000.0;
    let vertex_rate = fps * total_vertices as f64;

    println!("Rust Benchmark Results ({} frames):", iterations);
    println!("  Total time:  {:.2} ms", total_ms);
    println!("  Mean frame:  {:.3} us ({:.1} FPS)", mean_us, fps);
    println!("  Vertex rate: {:.3} M vertices/sec", vertex_rate / 1e6);

    println!("Standalone Rust timing only; no cross-language performance gate is claimed.");
}
