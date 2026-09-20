use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use kasane_core::{
    evaluate_frame, BindingAxis, Canvas, Document, DrawableFrame, ImageAsset, Mesh, MeshBinding,
    MeshKeyform, Parameter, Part, RotationPose, Transform, TransformKind, Vec2, VertexId,
    VertexPositionUpdate,
};

fn make_id(kind: u32, n: u32) -> String {
    format!("{:04x}{:04x}-4111-4111-8111-111111111111", kind, n)
}

fn build_production_model(out_params: &mut Vec<String>) -> Document {
    let mut doc = Document::new();
    let doc_id = make_id(0x0001, 1);
    assert!(doc
        .initialize(
            doc_id,
            Canvas::new(3840.0, 2160.0, Vec2::new(1920.0, 1080.0), 100.0)
        )
        .is_ok());

    // 4 Assets
    let mut asset_ids = Vec::new();
    for a in 0..4 {
        let aid = make_id(0x0002, a + 1);
        asset_ids.push(aid.clone());
        assert!(doc
            .add_asset(ImageAsset {
                id: aid,
                name: format!("texture_{a}"),
                source: format!("assets/{a}.png"),
                width: 2048,
                height: 2048,
                ..Default::default()
            })
            .status
            .is_ok());
    }

    // 8 Parameters (covering angles, eye tracking, breathing, physics)
    out_params.clear();
    for i in 0..8 {
        let pid = make_id(0x0003, i + 1);
        out_params.push(pid.clone());
        assert!(doc
            .create_parameter(Parameter {
                id: pid,
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

    // Root Part
    let root_part = make_id(0x0004, 1);
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

    // 5 Levels of Nested Deformers:
    // L1: Root Rotation
    // L2: Parent Warp (3x3 grid = 16 points)
    // L3: Mid Rotation
    // L4: Mid Warp (3x3 grid)
    // L5: Child Warp (3x3 grid)
    let rot_l1 = make_id(0x0005, 1);
    assert!(doc
        .create_transform(Transform {
            id: rot_l1.clone(),
            runtime_id: "Rot_L1".to_string(),
            name: "L1 Root Rotation".to_string(),
            part_id: root_part.clone(),
            parent_id: String::new(),
            kind: TransformKind::Rotation,
            rotation: RotationPose {
                origin: Vec2::new(1920.0, 1080.0),
                angle: 0.0,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
            ..Default::default()
        })
        .status
        .is_ok());

    let warp_l2 = make_id(0x0005, 2);
    let mut tfm_warp_l2 = Transform {
        id: warp_l2.clone(),
        runtime_id: "Warp_L2".to_string(),
        name: "L2 Parent Warp".to_string(),
        part_id: root_part.clone(),
        parent_id: rot_l1.clone(),
        kind: TransformKind::Warp,
        rows: 3,
        columns: 3,
        quad: true,
        points: Vec::new(),
        ..Default::default()
    };
    for r in 0..=3 {
        for c in 0..=3 {
            tfm_warp_l2.points.push(Vec2::new(
                c as f32 * 100.0 - 150.0,
                r as f32 * 100.0 - 150.0,
            ));
        }
    }
    assert!(doc.create_transform(tfm_warp_l2).status.is_ok());

    let rot_l3 = make_id(0x0005, 3);
    assert!(doc
        .create_transform(Transform {
            id: rot_l3.clone(),
            runtime_id: "Rot_L3".to_string(),
            name: "L3 Mid Rotation".to_string(),
            part_id: root_part.clone(),
            parent_id: warp_l2.clone(),
            kind: TransformKind::Rotation,
            rotation: RotationPose {
                origin: Vec2::new(1920.0, 1080.0),
                angle: 0.0,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
            ..Default::default()
        })
        .status
        .is_ok());

    let warp_l4 = make_id(0x0005, 4);
    let mut tfm_warp_l4 = Transform {
        id: warp_l4.clone(),
        runtime_id: "Warp_L4".to_string(),
        name: "L4 Mid Warp".to_string(),
        part_id: root_part.clone(),
        parent_id: rot_l3.clone(),
        kind: TransformKind::Warp,
        rows: 3,
        columns: 3,
        quad: true,
        points: Vec::new(),
        ..Default::default()
    };
    for r in 0..=3 {
        for c in 0..=3 {
            tfm_warp_l4
                .points
                .push(Vec2::new(c as f32 * 80.0 - 120.0, r as f32 * 80.0 - 120.0));
        }
    }
    assert!(doc.create_transform(tfm_warp_l4).status.is_ok());

    let warp_l5 = make_id(0x0005, 5);
    let mut tfm_warp_l5 = Transform {
        id: warp_l5.clone(),
        runtime_id: "Warp_L5".to_string(),
        name: "L5 Child Warp".to_string(),
        part_id: root_part.clone(),
        parent_id: warp_l4.clone(),
        kind: TransformKind::Warp,
        rows: 3,
        columns: 3,
        quad: true,
        points: Vec::new(),
        ..Default::default()
    };
    for r in 0..=3 {
        for c in 0..=3 {
            tfm_warp_l5
                .points
                .push(Vec2::new(c as f32 * 60.0 - 90.0, r as f32 * 60.0 - 90.0));
        }
    }
    assert!(doc.create_transform(tfm_warp_l5).status.is_ok());

    // 80 Meshes: Each with a 10x10 vertex grid = 100 vertices, 162 triangles.
    // Total = 80 * 100 = 8,000 vertices, 12,960 triangles!
    let deformers = [warp_l2, rot_l3, warp_l4, warp_l5];

    for m in 0..80 {
        let mesh_id = make_id(0x0006, m + 1);
        let def_id = deformers[m as usize % deformers.len()].clone();
        let asset_id = asset_ids[m as usize % asset_ids.len()].clone();

        let mut mesh = Mesh {
            id: mesh_id.clone(),
            runtime_id: format!("Mesh_{m}"),
            name: format!("Mesh {m}"),
            texture_asset_id: asset_id,
            part_id: root_part.clone(),
            deformer_id: def_id,
            draw_order: Some(m as f32 * 0.1),
            ..Default::default()
        };

        // 10x10 vertex grid
        let mut vid: VertexId = 0;
        for r in 0..10 {
            for c in 0..10 {
                mesh.vertex_ids.push(vid);
                vid += 1;
                mesh.base_positions.push(Vec2::new(
                    (c as f32 - 4.5) * 15.0 + (m as f32 % 8.0) * 40.0 - 140.0,
                    (r as f32 - 4.5) * 15.0 + (m as f32 / 8.0) * 40.0 - 200.0,
                ));
                mesh.uvs.push(Vec2::new(c as f32 / 9.0, r as f32 / 9.0));
            }
        }
        for r in 0..9 {
            for c in 0..9 {
                let v0 = (r * 10 + c) as VertexId;
                let v1 = (r * 10 + c + 1) as VertexId;
                let v2 = ((r + 1) * 10 + c) as VertexId;
                let v3 = ((r + 1) * 10 + c + 1) as VertexId;
                mesh.triangles.push([v0, v1, v2]);
                mesh.triangles.push([v1, v3, v2]);
            }
        }
        assert!(doc.create_mesh(mesh.clone()).status.is_ok());

        // Multi-axis 2D keyforms on mesh (Params 0 and 1)
        if m % 2 == 0 {
            let mut mb = MeshBinding {
                id: make_id(0x0007, m + 1),
                mesh_id: mesh_id.clone(),
                axes: vec![
                    BindingAxis {
                        parameter_id: out_params[0].clone(),
                        keys: vec![-1.0, 0.0, 1.0],
                    },
                    BindingAxis {
                        parameter_id: out_params[1].clone(),
                        keys: vec![-1.0, 1.0],
                    },
                ],
                keyforms: Vec::new(),
            };

            for &y in &[-1.0f32, 1.0] {
                for &x in &[-1.0f32, 0.0, 1.0] {
                    let mut kf = MeshKeyform {
                        keys: vec![x, y],
                        positions: mesh.base_positions.clone(),
                        ..Default::default()
                    };
                    for p in &mut kf.positions {
                        p.x += x * 8.0;
                        p.y += y * 6.0;
                    }
                    mb.keyforms.push(kf);
                }
            }
            assert!(doc.create_binding(mb).status.is_ok());
        }
    }

    doc
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn main() {
    println!("=== Kasane 2D Production Scale Benchmark ===");
    let mut param_ids = Vec::new();
    let mut doc = build_production_model(&mut param_ids);

    let total_vertices: usize = doc
        .mesh_order()
        .iter()
        .map(|id| doc.get_mesh(id).unwrap().base_positions.len())
        .sum();
    let total_triangles: usize = doc
        .mesh_order()
        .iter()
        .map(|id| doc.get_mesh(id).unwrap().triangles.len())
        .sum();

    println!("Model Scale:");
    println!("  Meshes:          {}", doc.mesh_order().len());
    println!("  Deformer Depth:  5 levels (nested Warps & Rotations)");
    println!("  Total Vertices:  {}", total_vertices);
    println!("  Total Triangles: {}", total_triangles);
    println!("  Parameters:      {}", param_ids.len());

    let mut frame = DrawableFrame::default();
    let mut preview = HashMap::new();

    // Warmup
    for i in 0..100 {
        let angle = i as f32 * 0.1;
        preview.insert(param_ids[0].clone(), angle.sin());
        preview.insert(param_ids[1].clone(), angle.cos());
        preview.insert(param_ids[2].clone(), (angle * 0.5).sin());
        assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    }

    // Workload 1: Continuous 60 FPS Playback Simulation (3,600 frames = 1 full minute)
    let playback_frames = 3600;
    println!(
        "\n[Workload 1] Continuous Playback Simulation ({} frames)...",
        playback_frames
    );

    let mut latencies_us = Vec::with_capacity(playback_frames);
    let playback_start = Instant::now();

    for f in 0..playback_frames {
        let t = f as f32 * 0.033; // 30 Hz animation wave across parameters
        preview.insert(param_ids[0].clone(), (t * 1.5).sin());
        preview.insert(param_ids[1].clone(), (t * 1.1).cos());
        preview.insert(param_ids[2].clone(), (t * 0.7).sin());
        preview.insert(param_ids[3].clone(), (t * 2.3).sin() * 0.5);

        let t0 = Instant::now();
        assert!(evaluate_frame(black_box(&doc), black_box(&preview), &mut frame).is_ok());
        black_box(&frame);
        let dt = t0.elapsed().as_secs_f64() * 1_000_000.0;
        latencies_us.push(dt);
    }

    let total_playback_time = playback_start.elapsed();
    latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let sum_us: f64 = latencies_us.iter().sum();
    let mean_us = sum_us / playback_frames as f64;
    let min_us = latencies_us[0];
    let p50_us = percentile(&latencies_us, 0.50);
    let p90_us = percentile(&latencies_us, 0.90);
    let p95_us = percentile(&latencies_us, 0.95);
    let p99_us = percentile(&latencies_us, 0.99);
    let max_us = latencies_us[latencies_us.len() - 1];

    let total_ms = total_playback_time.as_secs_f64() * 1000.0;
    let throughput_fps = (playback_frames as f64 / total_ms) * 1000.0;
    let m_vertices_per_sec = (throughput_fps * total_vertices as f64) / 1_000_000.0;

    println!("Continuous Playback Latency Distribution (microseconds):");
    println!("  Min:        {:>8.2} µs", min_us);
    println!("  p50 (med):  {:>8.2} µs", p50_us);
    println!("  p90:        {:>8.2} µs", p90_us);
    println!("  p95:        {:>8.2} µs", p95_us);
    println!("  p99:        {:>8.2} µs", p99_us);
    println!("  Max:        {:>8.2} µs", max_us);
    println!("  Mean:       {:>8.2} µs", mean_us);
    println!("\nThroughput & Budget:");
    println!(
        "  Achieved Framerate: {:.1} FPS (60 FPS budget = 16,666 µs)",
        throughput_fps
    );
    println!("  Headroom at 60 FPS: {:.2}x margin", 16666.67 / p99_us);
    println!(
        "  Vertex Throughput:  {:.2} Million vertices / sec",
        m_vertices_per_sec
    );

    // Workload 2: Interactive Scrubbing & Mutation Simulation (1,000 rapid vertex updates + eval)
    let scrub_frames = 1000;
    println!(
        "\n[Workload 2] Interactive Scrubbing & Vertex Drag Simulation ({} frames)...",
        scrub_frames
    );

    let target_mesh_id = doc.mesh_order()[0].clone();
    let mesh_vids = doc.get_mesh(&target_mesh_id).unwrap().vertex_ids.clone();
    let mut scrub_latencies_us = Vec::with_capacity(scrub_frames);
    let scrub_start = Instant::now();

    for f in 0..scrub_frames {
        let delta = (f as f32 * 0.1).sin() * 2.0;
        let update = VertexPositionUpdate {
            mesh_id: target_mesh_id.clone(),
            vertex_ids: vec![mesh_vids[0]],
            positions: vec![Vec2::new(delta, delta)],
        };

        let t0 = Instant::now();
        assert!(doc.apply_vertex_position_updates(&[update]).status.is_ok());
        preview.insert(param_ids[0].clone(), (f as f32 * 0.05).sin());
        assert!(evaluate_frame(black_box(&doc), black_box(&preview), &mut frame).is_ok());
        black_box(&frame);
        let dt = t0.elapsed().as_secs_f64() * 1_000_000.0;
        scrub_latencies_us.push(dt);
    }

    let total_scrub_ms = scrub_start.elapsed().as_secs_f64() * 1000.0;
    scrub_latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let scrub_p50 = percentile(&scrub_latencies_us, 0.50);
    let scrub_p95 = percentile(&scrub_latencies_us, 0.95);
    let scrub_p99 = percentile(&scrub_latencies_us, 0.99);

    println!("Interactive Edit + Eval Latency Distribution (microseconds):");
    println!("  p50:        {:>8.2} µs", scrub_p50);
    println!("  p95:        {:>8.2} µs", scrub_p95);
    println!("  p99:        {:>8.2} µs", scrub_p99);
    println!(
        "  Mean:       {:>8.2} µs ({:.1} updates/sec)",
        (total_scrub_ms * 1000.0) / scrub_frames as f64,
        (scrub_frames as f64 / total_scrub_ms) * 1000.0
    );

    println!("\n=== Production Scale Benchmark Completed ===");
}
