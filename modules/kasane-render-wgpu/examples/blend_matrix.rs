//! Standalone GPU matrix for the 18 color × 5 alpha extended blend modes.
//! The companion script `tools/compare_wgpu_blends.py` compares its PNG with
//! the existing Godot shader reference using identical sample colors.
use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kasane_core::evaluation::{Drawable, DrawableFrame, OffscreenFrame, RenderCommand};
use kasane_core::types::{Canvas, Status, Vec2};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_wgpu::{
    WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig, WgpuTexture,
    WgpuTextureCatalog,
};

const CELL: u32 = 8;
const WIDTH: u32 = 8 * CELL;
const HEIGHT: u32 = 18 * 5 * CELL;
const ROW_PITCH: u32 = 256;

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn renderer_result<T>(result: Result<T, Status>) -> Result<T, std::io::Error> {
    result.map_err(|status| std::io::Error::other(format!("{status:?}")))
}

fn rect(id: String, texture: &str, x: f32, y: f32) -> Drawable {
    let x1 = x + CELL as f32;
    let y1 = y + CELL as f32;
    Drawable {
        id,
        texture_asset_id: texture.to_owned(),
        positions: vec![
            Vec2::new(x, -y),
            Vec2::new(x1, -y),
            Vec2::new(x1, -y1),
            Vec2::new(x, -y1),
        ],
        uvs: Arc::from([
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]),
        indices: Arc::from([0, 1, 2, 0, 2, 3]),
        ..Default::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/wgpu-blend-matrix.png"));
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    // The Godot comparison fixture receives these as straight source colors
    // and premultiplied destination colors. The WGPU normal draw produces the
    // destination's premultiplied attachment value from straight input.
    let colors = [
        ("source0", [51, 153, 230, 128]),
        ("destination0", [204, 77, 26, 102]),
        ("source1", [230, 51, 153, 204]),
        ("destination1", [26, 179, 77, 255]),
        ("maskhalf", [255, 255, 255, 128]),
        ("maskinverse", [255, 255, 255, 127]),
        ("source2", [0, 0, 0, 255]),
        ("destination2", [255, 255, 255, 255]),
        ("source3", [255, 255, 255, 255]),
        ("destination3", [0, 0, 0, 255]),
        ("source4", [240, 80, 160, 0]),
        ("destination4", [80, 240, 160, 128]),
        ("source5", [240, 80, 160, 128]),
        ("destination5", [0, 0, 0, 1]),
    ];
    let sources: Vec<_> = colors
        .iter()
        .map(|(name, rgba)| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(name),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            texture
        })
        .collect();
    let views: Vec<_> = sources
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let textures = WgpuTextureCatalog::new(
        colors
            .iter()
            .zip(&views)
            .map(|((name, _), view)| {
                (
                    (*name).to_owned(),
                    WgpuTexture {
                        view,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect::<HashMap<_, _>>(),
    );

    let mut frame = DrawableFrame {
        canvas: Canvas::new(WIDTH as f32, HEIGHT as f32, Vec2::default(), 1.0),
        ..Default::default()
    };
    for color in 0..18_u32 {
        for alpha in 0..5_u32 {
            let row = color * 5 + alpha;
            let y = (row * CELL) as f32;
            let blend = color | (alpha << 8);
            for sample in 0..8_u32 {
                let x = (sample * CELL) as f32;
                let offscreen = sample % 2 == 1;
                let masked = sample == 2 || sample == 3;
                let texture_pair = match sample {
                    0 | 2 => 0,
                    1 | 3 => 1,
                    4 => 2,
                    5 => 3,
                    6 => 4,
                    7 => 5,
                    _ => unreachable!(),
                };
                let source_texture = format!("source{texture_pair}");
                let destination_texture = format!("destination{texture_pair}");
                let mask_id = format!("mask-{row}-{sample}");
                if masked {
                    let mut mask_source = rect(
                        mask_id.clone(),
                        if offscreen { "maskinverse" } else { "maskhalf" },
                        x,
                        y,
                    );
                    mask_source.visible = false;
                    frame.drawables.push(mask_source);
                    frame.render_plan.push(RenderCommand::DrawMesh {
                        mesh_id: mask_id.clone(),
                    });
                }
                let destination_id = format!("destination-{row}-{sample}");
                frame
                    .drawables
                    .push(rect(destination_id.clone(), &destination_texture, x, y));
                frame.render_plan.push(RenderCommand::DrawMesh {
                    mesh_id: destination_id,
                });
                let source_id = format!("source-{row}-{sample}");
                let mut source = rect(source_id.clone(), &source_texture, x, y);
                if offscreen {
                    let group = format!("group-{row}-{sample}");
                    frame.drawables.push(source);
                    frame.offscreens.push(OffscreenFrame {
                        id: group.clone(),
                        enabled: true,
                        opacity: if sample == 1 || sample == 3 || sample == 7 {
                            0.4
                        } else {
                            1.0
                        },
                        blend_mode: blend,
                        flags: if masked { 8 } else { 0 },
                        masks: if masked { vec![mask_id] } else { Vec::new() },
                        multiply_color: [1.0; 4],
                        ..Default::default()
                    });
                    frame.render_plan.push(RenderCommand::BeginOffscreen {
                        offscreen_id: group.clone(),
                    });
                    frame
                        .render_plan
                        .push(RenderCommand::DrawMesh { mesh_id: source_id });
                    frame.render_plan.push(RenderCommand::EndOffscreen {
                        offscreen_id: group,
                    });
                } else {
                    source.raw_blend_mode = Some(blend);
                    source.opacity = if sample == 2 { 0.7 } else { 1.0 };
                    if masked {
                        source.masks.push(mask_id);
                    }
                    frame.drawables.push(source);
                    frame
                        .render_plan
                        .push(RenderCommand::DrawMesh { mesh_id: source_id });
                }
            }
        }
    }
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blend-matrix.output"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut renderer = renderer_result(WgpuRenderer::new(
        &device,
        WgpuTargetConfig {
            width: WIDTH,
            height: HEIGHT,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    ))?;
    renderer_result(renderer.sync_model(&device, &frame, &textures))?;
    drop(frame);
    renderer_result(renderer.update_view(
        &device,
        ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(WIDTH as f32, HEIGHT as f32),
            mask_scale: 1.0,
        },
    ))?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blend-matrix.encoder"),
    });
    let stats = renderer_result(renderer.encode(
        WgpuEncodeTarget {
            device: &device,
            queue: &queue,
            encoder: &mut encoder,
            output: &output_view,
            output_mode: WgpuOutputMode::Replace,
        },
        &textures,
    ))?;
    queue.submit([encoder.finish()]);

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blend-matrix.readback"),
        size: u64::from(ROW_PITCH * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blend-matrix.readback.encoder"),
    });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).expect("readback receiver");
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    receiver.recv_timeout(Duration::from_secs(1))??;
    let mapped = readback.get_mapped_range(..);
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in mapped.as_chunks::<{ ROW_PITCH as usize }>().0 {
        pixels.extend_from_slice(&row[..(WIDTH * 4) as usize]);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut png = png::Encoder::new(File::create(&path)?, WIDTH, HEIGHT);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(&pixels)?;
    println!("{}: {stats:?}", path.display());
    Ok(())
}
