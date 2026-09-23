use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use kasane_core::evaluation::{Drawable, DrawableFrame, OffscreenFrame, RenderCommand};
use kasane_core::types::{Canvas, Vec2};
use kasane_render::{build_plan, Affine2, TextureInfo, ViewportConfig};

const WARMUP_ITERATIONS: usize = 100;
const SAMPLE_ITERATIONS: usize = 2_000;

struct Workload {
    name: &'static str,
    frame: DrawableFrame,
    textures: HashMap<String, TextureInfo>,
}

fn make_workload(
    name: &'static str,
    drawable_count: usize,
    offscreen_count: usize,
    mask_count: usize,
) -> Workload {
    let mut drawables = Vec::with_capacity(drawable_count);
    for index in 0..drawable_count {
        drawables.push(Drawable {
            id: format!("mesh-{index}"),
            runtime_id: format!("Mesh{index}"),
            texture_asset_id: format!("texture-{}", index % 4),
            positions: vec![
                Vec2::new(-20.0, -20.0),
                Vec2::new(20.0, -20.0),
                Vec2::new(0.0, 20.0),
            ],
            uvs: Arc::from([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.5, 1.0),
            ]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        });
    }

    let mask_count = mask_count.min(drawable_count.saturating_sub(1));
    for index in 1..=mask_count {
        drawables[index].masks.push("mesh-0".to_owned());
    }

    let offscreen_count = offscreen_count.min(drawable_count);
    let mut offscreens = Vec::with_capacity(offscreen_count);
    let mut render_plan = Vec::with_capacity(offscreen_count * 3 + drawable_count);
    for index in 0..offscreen_count {
        let id = format!("offscreen-{index}");
        render_plan.push(RenderCommand::BeginOffscreen {
            offscreen_id: id.clone(),
        });
        render_plan.push(RenderCommand::DrawMesh {
            mesh_id: format!("mesh-{index}"),
        });
        render_plan.push(RenderCommand::EndOffscreen {
            offscreen_id: id.clone(),
        });
        offscreens.push(OffscreenFrame {
            id,
            runtime_id: format!("Offscreen{index}"),
            enabled: true,
            opacity: 1.0,
            ..Default::default()
        });
    }
    for index in offscreen_count..drawable_count {
        render_plan.push(RenderCommand::DrawMesh {
            mesh_id: format!("mesh-{index}"),
        });
    }

    let textures = (0..4)
        .map(|index| {
            (
                format!("texture-{index}"),
                TextureInfo {
                    width: 2048,
                    height: 2048,
                },
            )
        })
        .collect();

    Workload {
        name,
        frame: DrawableFrame {
            canvas: Canvas::new(3840.0, 2160.0, Vec2::new(1920.0, 1080.0), 100.0),
            drawables,
            offscreens,
            render_plan,
            ..Default::default()
        },
        textures,
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() as f64 - 1.0) * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn measure(workload: &Workload) -> (f64, f64, f64, f64) {
    let viewport = ViewportConfig {
        transform: Affine2 {
            a: Vec2::new(0.5, 0.0),
            b: Vec2::new(0.0, 0.5),
            origin: Vec2::new(32.25, 18.75),
        },
        target_extent: Vec2::new(1920.0, 1080.0),
        mask_scale: 0.5,
    };

    for _ in 0..WARMUP_ITERATIONS {
        let plan = build_plan(
            black_box(&workload.frame),
            black_box(&workload.textures),
            viewport,
        )
        .expect("benchmark workload must be valid");
        black_box(plan);
    }

    let mut samples = Vec::with_capacity(SAMPLE_ITERATIONS);
    let total_start = Instant::now();
    for _ in 0..SAMPLE_ITERATIONS {
        let start = Instant::now();
        let plan = build_plan(
            black_box(&workload.frame),
            black_box(&workload.textures),
            viewport,
        )
        .expect("benchmark workload must be valid");
        black_box(plan);
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let total_us = total_start.elapsed().as_secs_f64() * 1_000_000.0;
    samples.sort_by(|a, b| a.partial_cmp(b).expect("finite sample"));
    let mean_us = total_us / SAMPLE_ITERATIONS as f64;
    (
        mean_us,
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99),
    )
}

fn main() {
    let workloads = [
        make_workload("small_flat", 10, 0, 0),
        make_workload("production_offscreen", 80, 16, 16),
        make_workload("mask_heavy", 80, 16, 64),
    ];

    println!("=== Kasane render planning benchmark ===");
    println!("build_profile=release warmup={WARMUP_ITERATIONS} samples={SAMPLE_ITERATIONS}");
    println!("workload,mean_us,p50_us,p95_us,p99_us");
    let mut results = Vec::new();
    for workload in &workloads {
        let (mean_us, p50_us, p95_us, p99_us) = measure(workload);
        println!(
            "{},{mean_us:.3},{p50_us:.3},{p95_us:.3},{p99_us:.3}",
            workload.name
        );
        results.push(format!(
            "{{\"name\":\"{}\",\"mean_us\":{mean_us:.6},\"p50_us\":{p50_us:.6},\"p95_us\":{p95_us:.6},\"p99_us\":{p99_us:.6}}}",
            workload.name
        ));
    }
    println!(
        "BENCHMARK_RESULT {{\"schema_version\":1,\"benchmark\":\"kasane-render-plan\",\"results\":[{}]}}",
        results.join(",")
    );
}
