//! End-to-end Kasane CPU evaluation and native Metal rendering of the matrix workload.

use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use kasane_core::evaluation::RenderCommand;
use kasane_core::{Canvas, DrawableFrame, PreviewValues, Vec2};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer, MetalTargetConfig,
    MetalTexture, MetalTextureCatalog,
};
use kasane_sdk::{AuthoringSession, MotionPreview};
use kasane_sdk_observe::ObservationInput;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000001";
const GEOMETRY_PHASES: [&str; 12] = [
    "workspace_setup",
    "preflight",
    "parameters",
    "selections",
    "parts",
    "transforms",
    "meshes",
    "glue",
    "render_plan",
    "part_opacity",
    "model_opacity",
    "unattributed",
];

#[derive(Deserialize)]
struct Workload {
    id: String,
    model: String,
    instances: usize,
    layout: Layout,
    viewport: [u32; 2],
    timing: Timing,
    motion: Motion,
}

#[derive(Deserialize)]
struct Layout {
    columns: usize,
    rows: usize,
    cell_fill: f32,
}

#[derive(Deserialize)]
struct Timing {
    warmup_seconds: f64,
    sample_seconds: f64,
}

#[derive(Deserialize)]
struct Motion {
    group: String,
    index: usize,
}

fn prefixed(instance: usize, id: &str) -> String {
    format!("{instance}:{id}")
}

/// Put independent evaluated models in one render plan so all drawables share
/// one output pass. This assembly is deliberately included in frame timing.
fn assemble(frames: Vec<DrawableFrame>, workload: &Workload) -> DrawableFrame {
    let width = workload.viewport[0] as f32;
    let height = workload.viewport[1] as f32;
    let cell_width = width / workload.layout.columns as f32;
    let cell_height = height / workload.layout.rows as f32;
    let mut scene = DrawableFrame {
        canvas: Canvas::new(width, height, Vec2::new(0.0, 0.0), 1.0),
        ..Default::default()
    };
    for (instance, mut frame) in frames.into_iter().enumerate() {
        let canvas = frame.canvas;
        let scale = (cell_width * workload.layout.cell_fill / canvas.width)
            .min(cell_height * workload.layout.cell_fill / canvas.height);
        let left = (instance % workload.layout.columns) as f32 * cell_width
            + (cell_width - canvas.width * scale) * 0.5;
        let top = (instance / workload.layout.columns) as f32 * cell_height
            + (cell_height - canvas.height * scale) * 0.5;
        for drawable in &mut frame.drawables {
            drawable.id = prefixed(instance, &drawable.id);
            drawable.part_id = prefixed(instance, &drawable.part_id);
            drawable.masks = drawable
                .masks
                .iter()
                .map(|id| prefixed(instance, id))
                .collect();
            for position in &mut drawable.positions {
                let x = (position.x * canvas.pixels_per_unit + canvas.origin.x) * scale + left;
                let y = (canvas.origin.y - position.y * canvas.pixels_per_unit) * scale + top;
                *position = Vec2::new(x, -y);
            }
        }
        for offscreen in &mut frame.offscreens {
            offscreen.id = prefixed(instance, &offscreen.id);
            offscreen.owner_part_id = prefixed(instance, &offscreen.owner_part_id);
            offscreen.parent_offscreen_id = offscreen
                .parent_offscreen_id
                .as_ref()
                .map(|id| prefixed(instance, id));
            offscreen.masks = offscreen
                .masks
                .iter()
                .map(|id| prefixed(instance, id))
                .collect();
        }
        scene
            .render_plan
            .extend(frame.render_plan.into_iter().map(|command| match command {
                RenderCommand::BeginOffscreen { offscreen_id } => RenderCommand::BeginOffscreen {
                    offscreen_id: prefixed(instance, &offscreen_id),
                },
                RenderCommand::DrawMesh { mesh_id } => RenderCommand::DrawMesh {
                    mesh_id: prefixed(instance, &mesh_id),
                },
                RenderCommand::EndOffscreen { offscreen_id } => RenderCommand::EndOffscreen {
                    offscreen_id: prefixed(instance, &offscreen_id),
                },
            }));
        scene.drawables.extend(frame.drawables);
        scene.offscreens.extend(frame.offscreens);
    }
    scene
}

fn mipmaps(mut width: u32, mut height: u32, rgba: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut source = rgba.to_vec();
    let mut levels = Vec::new();
    while width > 1 || height > 1 {
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let mut next = vec![0; (next_width * next_height * 4) as usize];
        for y in 0..next_height {
            for x in 0..next_width {
                for channel in 0..4 {
                    let mut sum = 0u32;
                    let mut count = 0u32;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = x * 2 + dx;
                            let sy = y * 2 + dy;
                            if sx < width && sy < height {
                                sum +=
                                    u32::from(source[((sy * width + sx) * 4 + channel) as usize]);
                                count += 1;
                            }
                        }
                    }
                    next[((y * next_width + x) * 4 + channel) as usize] =
                        ((sum + count / 2) / count) as u8;
                }
            }
        }
        source = next.clone();
        width = next_width;
        height = next_height;
        levels.push((width, height, next));
    }
    levels
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() as f64 * fraction).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(path.parent().ok_or("missing output directory")?)?;
    let mut encoder = png::Encoder::new(File::create(path)?, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

pub fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let workload_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: cubism-matrix-kasane WORKLOAD MODEL3")?,
    );
    let model_path = PathBuf::from(arguments.next().ok_or("missing MODEL3 path")?);
    let workload: Workload = serde_json::from_slice(&fs::read(&workload_path)?)?;
    if workload.instances == 0
        || workload.layout.columns * workload.layout.rows != workload.instances
        || workload.viewport.contains(&0)
        || !(0.0..=1.0).contains(&workload.layout.cell_fill)
        || workload.timing.sample_seconds <= 0.0
        || workload.timing.warmup_seconds < 0.0
    {
        return Err("invalid benchmark workload".into());
    }
    let model_hash = format!("{:x}", Sha256::digest(fs::read(&model_path)?));
    let mut session =
        AuthoringSession::new(DOCUMENT_ID, Canvas::new(1.0, 1.0, Vec2::new(0.0, 0.0), 1.0))?;
    let receipt = session.import_model3(&model_path, None)?;
    // Mao includes references to virtual/absent parameter and Part IDs. The
    // official runtime also accepts the package; retain this coverage signal.
    let import_diagnostics = receipt.project.diagnostics.len();
    if import_diagnostics != 0 {
        eprintln!("Kasane import reported {import_diagnostics} unresolved model3 references");
    }
    let observation = ObservationInput::capture(&session, &PreviewValues::new())?;
    let resolved = observation.resolve_textures()?;
    let context = MetalContext::new()?;
    let uploaded = resolved
        .iter()
        .map(|texture| {
            let levels = mipmaps(texture.data.width, texture.data.height, &texture.data.rgba);
            context
                .upload_rgba8(
                    texture.data.width,
                    texture.data.height,
                    &texture.data.rgba,
                    &levels,
                )
                .map(|gpu| {
                    (
                        texture.asset.id.clone(),
                        gpu,
                        texture.data.width,
                        texture.data.height,
                    )
                })
                .map_err(|status| format!("{status:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut catalog = MetalTextureCatalog::new(
        uploaded
            .iter()
            .map(|(id, gpu, width, height)| {
                (
                    id.clone(),
                    MetalTexture {
                        view: gpu,
                        width: *width,
                        height: *height,
                    },
                )
            })
            .collect::<HashMap<_, _>>(),
    );
    for (id, _, _, _) in &uploaded {
        catalog.set_repeat(id.clone(), true);
        catalog.set_revision(id.clone(), 1);
    }

    let mut previews: Vec<MotionPreview> = (0..workload.instances)
        .map(|_| session.motion_preview())
        .collect();
    for preview in &mut previews {
        preview.schedule_motion_entry(&workload.motion.group, workload.motion.index, 0.0)?;
    }
    let target = MetalTargetConfig {
        width: workload.viewport[0],
        height: workload.viewport[1],
        format: metal::MTLPixelFormat::RGBA8Unorm,
    };
    let output = context
        .output_texture(target)
        .map_err(|s| format!("{s:?}"))?;
    let mut renderer = MetalRenderer::new(&context, target).map_err(|s| format!("{s:?}"))?;
    let viewport = ViewportConfig {
        transform: Affine2::IDENTITY,
        target_extent: Vec2::new(target.width as f32, target.height as f32),
        mask_scale: 1.0,
    };
    println!(
        "BENCHMARK_READY case=kasane-metal renderer=metal models={} size={}x{} mipmaps=on warmup_s={} sample_s={}",
        workload.instances, target.width, target.height,
        workload.timing.warmup_seconds, workload.timing.sample_seconds,
    );

    let benchmark_start = Instant::now();
    let mut previous_frame = benchmark_start;
    let mut sample_start = None;
    let mut frame_times_ms = Vec::new();
    let mut evaluation_ms = Vec::new();
    let mut animation_update_ms = Vec::new();
    let mut geometry_evaluation_ms = Vec::new();
    let mut geometry_phase_ms: [Vec<f64>; GEOMETRY_PHASES.len()] =
        std::array::from_fn(|_| Vec::new());
    let mut assembly_ms = Vec::new();
    let mut sync_ms = Vec::new();
    let mut encode_ms = Vec::new();
    let mut gpu_wait_ms = Vec::new();
    loop {
        let frame_start = Instant::now();
        let dt = frame_start.duration_since(previous_frame).as_secs_f32();
        previous_frame = frame_start;
        let mut frames = Vec::with_capacity(workload.instances);
        let mut animation_time = 0.0;
        let mut geometry_time = 0.0;
        let mut phases = [0.0f64; GEOMETRY_PHASES.len()];
        for preview in &mut previews {
            let animation_start = Instant::now();
            preview.advance(dt)?;
            if preview.snapshot().active_motions.is_empty() {
                preview.schedule_motion_entry(
                    &workload.motion.group,
                    workload.motion.index,
                    preview.snapshot().time,
                )?;
            }
            let geometry_start = Instant::now();
            animation_time += geometry_start.duration_since(animation_start).as_secs_f64() * 1000.0;
            let (mut frame, timings) = preview.evaluate_drawables_timed()?;
            let ms = |duration: std::time::Duration| duration.as_secs_f64() * 1000.0;
            phases[0] += ms(timings.workspace_setup);
            phases[1] += ms(timings.core.preflight);
            phases[2] += ms(timings.core.parameters);
            phases[3] += ms(timings.core.selections);
            phases[4] += ms(timings.core.parts);
            phases[5] += ms(timings.core.transforms);
            phases[6] += ms(timings.core.meshes);
            phases[7] += ms(timings.core.glue);
            phases[8] += ms(timings.core.render);
            phases[9] += ms(timings.part_opacity);
            let opacity = preview.snapshot().model_opacity;
            let opacity_start = Instant::now();
            for drawable in &mut frame.drawables {
                drawable.opacity *= opacity;
            }
            phases[10] += opacity_start.elapsed().as_secs_f64() * 1000.0;
            frames.push(frame);
            geometry_time += geometry_start.elapsed().as_secs_f64() * 1000.0;
        }
        phases[11] = geometry_time - phases[..11].iter().sum::<f64>();
        let evaluated_at = Instant::now();
        let scene = assemble(frames, &workload);
        let assembled_at = Instant::now();
        renderer
            .sync_model_shared(Arc::new(scene), &catalog)
            .map_err(|s| format!("{s:?}"))?;
        let synced_at = Instant::now();
        renderer
            .update_view(viewport)
            .map_err(|s| format!("{s:?}"))?;
        // This CLI has no AppKit event loop to drain autoreleased commands.
        let command =
            metal::objc::rc::autoreleasepool(|| context.queue().new_command_buffer().to_owned());
        let stats = renderer
            .encode(&command, &output, MetalOutputMode::Replace, &catalog)
            .map_err(|s| format!("{s:?}"))?;
        let encoded_at = Instant::now();
        command.commit();
        command.wait_until_completed();
        if command.status() != metal::MTLCommandBufferStatus::Completed {
            return Err("Metal command buffer failed".into());
        }
        let now = Instant::now();
        if let Some(start) = sample_start {
            frame_times_ms.push(now.duration_since(frame_start).as_secs_f64() * 1000.0);
            evaluation_ms.push(evaluated_at.duration_since(frame_start).as_secs_f64() * 1000.0);
            animation_update_ms.push(animation_time);
            geometry_evaluation_ms.push(geometry_time);
            for (samples, value) in geometry_phase_ms.iter_mut().zip(phases) {
                samples.push(value);
            }
            assembly_ms.push(assembled_at.duration_since(evaluated_at).as_secs_f64() * 1000.0);
            sync_ms.push(synced_at.duration_since(assembled_at).as_secs_f64() * 1000.0);
            encode_ms.push(encoded_at.duration_since(synced_at).as_secs_f64() * 1000.0);
            gpu_wait_ms.push(now.duration_since(encoded_at).as_secs_f64() * 1000.0);
            if now.duration_since(start).as_secs_f64() >= workload.timing.sample_seconds {
                let elapsed = now.duration_since(start).as_secs_f64();
                frame_times_ms.sort_by(f64::total_cmp);
                evaluation_ms.sort_by(f64::total_cmp);
                animation_update_ms.sort_by(f64::total_cmp);
                geometry_evaluation_ms.sort_by(f64::total_cmp);
                for samples in &mut geometry_phase_ms {
                    samples.sort_by(f64::total_cmp);
                }
                assembly_ms.sort_by(f64::total_cmp);
                sync_ms.sort_by(f64::total_cmp);
                encode_ms.sort_by(f64::total_cmp);
                gpu_wait_ms.sort_by(f64::total_cmp);
                let pixels = read_rgba8(&output).map_err(|s| format!("{s:?}"))?;
                let visible_pixels = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[3] != 0)
                    .count();
                if visible_pixels == 0 {
                    return Err("Kasane produced an empty frame".into());
                }
                let geometry_breakdown = GEOMETRY_PHASES
                    .into_iter()
                    .zip(&geometry_phase_ms)
                    .map(|(name, samples)| (name.to_owned(), json!(percentile(samples, 0.50))))
                    .collect::<serde_json::Map<_, _>>();
                let result = json!({
                    "schema_version": 1,
                    "workload_id": workload.id,
                    "case_id": "kasane-metal",
                    "core_backend": "kasane",
                    "host_backend": "kasane-render-metal",
                    "graphics_api": "metal",
                    "build_profile": "release",
                    "model": workload.model,
                    "model_hash": model_hash,
                    "instances": workload.instances,
                    "viewport": workload.viewport,
                    "texture_mipmaps": true,
                    "warmup_seconds": workload.timing.warmup_seconds,
                    "sample_seconds": elapsed,
                    "frames": frame_times_ms.len(),
                    "average_fps": frame_times_ms.len() as f64 / elapsed,
                    "p50_frame_ms": percentile(&frame_times_ms, 0.50),
                    "p95_frame_ms": percentile(&frame_times_ms, 0.95),
                    "p99_frame_ms": percentile(&frame_times_ms, 0.99),
                    "p50_animation_evaluation_ms": percentile(&evaluation_ms, 0.50),
                    "p50_animation_update_ms": percentile(&animation_update_ms, 0.50),
                    "p50_geometry_evaluation_ms": percentile(&geometry_evaluation_ms, 0.50),
                    "p50_geometry_breakdown_ms": geometry_breakdown,
                    "p50_scene_assembly_ms": percentile(&assembly_ms, 0.50),
                    "p50_metal_sync_ms": percentile(&sync_ms, 0.50),
                    "p50_metal_encode_ms": percentile(&encode_ms, 0.50),
                    "p50_gpu_wait_ms": percentile(&gpu_wait_ms, 0.50),
                    "visible_pixels": visible_pixels,
                    "import_diagnostics": import_diagnostics,
                    "draw_calls": stats.draw_calls,
                    "render_passes": stats.render_passes,
                    "mask_count": stats.masks,
                    "timing_scope": "animation+evaluation+scene_assembly+metal_sync+encode+gpu_wait",
                    "presentation": "offscreen",
                    "gpu_synchronization": "wait_until_completed_each_frame",
                    "layout": {"columns": workload.layout.columns, "rows": workload.layout.rows, "cell_fill": workload.layout.cell_fill},
                    "motion": {"group": workload.motion.group, "index": workload.motion.index},
                    "mask_policy": "canvas_pixel_density_1_with_4px_padding",
                });
                println!("BENCHMARK_RESULT {result}");
                let results = workload_path
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("artifacts/results");
                fs::create_dir_all(&results)?;
                fs::write(
                    results.join("latest-kasane-metal.json"),
                    serde_json::to_vec_pretty(&result)?,
                )?;
                save_png(
                    &results.join("latest-kasane-metal.png"),
                    target.width,
                    target.height,
                    &pixels,
                )?;
                break;
            }
        } else if now.duration_since(benchmark_start).as_secs_f64()
            >= workload.timing.warmup_seconds
        {
            sample_start = Some(now);
            println!("BENCHMARK_SAMPLE_BEGIN");
        }
    }
    Ok(())
}
