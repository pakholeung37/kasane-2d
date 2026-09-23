//! Minimal standalone WGPU host. Run with:
//! cargo run -p kasane-render-wgpu --example offscreen -- target/wgpu-minimal.png
use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kasane_core::evaluation::{Drawable, DrawableFrame};
use kasane_core::types::{Canvas, Status, Vec2};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_wgpu::{
    WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig, WgpuTexture,
    WgpuTextureCatalog,
};

const SIDE: u32 = 64;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/wgpu-minimal.png"));
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;

    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("minimal.source"),
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
        source.as_image_copy(),
        &[255, 40, 20, 255],
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
    let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
    let textures = WgpuTextureCatalog::new(HashMap::from([(
        "solid".to_owned(),
        WgpuTexture {
            view: &source_view,
            width: 1,
            height: 1,
        },
    )]));
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("minimal.output"),
        size: wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
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
    let frame = DrawableFrame {
        canvas: Canvas::new(SIDE as f32, SIDE as f32, Vec2::new(0.0, 0.0), 1.0),
        drawables: vec![Drawable {
            id: "triangle".into(),
            texture_asset_id: "solid".into(),
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(SIDE as f32, 0.0),
                Vec2::new(0.0, -(SIDE as f32)),
            ],
            uvs: Arc::from([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut renderer = renderer_result(WgpuRenderer::new(
        &device,
        WgpuTargetConfig {
            width: SIDE,
            height: SIDE,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    ))?;
    renderer_result(renderer.sync_model(&device, &frame, &textures))?;
    drop(frame);
    renderer_result(renderer.update_view(
        &device,
        ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(SIDE as f32, SIDE as f32),
            mask_scale: 1.0,
        },
    ))?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("minimal.encoder"),
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

    // Rgba8 uses 4 bytes per pixel, so 64 pixels make the required 256-byte row.
    let row_bytes = SIDE * 4;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("minimal.readback"),
        size: u64::from(row_bytes * SIDE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("minimal.readback.encoder"),
    });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(SIDE),
            },
        },
        wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
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
        timeout: Some(Duration::from_secs(10)),
    })?;
    receiver.recv_timeout(Duration::from_secs(1))??;
    let pixels = readback.get_mapped_range(..);
    let pixel = |x: usize, y: usize| &pixels[(y * SIDE as usize + x) * 4..][..4];
    assert_eq!(pixel(8, 8), &[255, 40, 20, 255]);
    assert_eq!(pixel(60, 60), &[0, 0, 0, 0]);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut png = png::Encoder::new(File::create(&path)?, SIDE, SIDE);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(&pixels)?;
    println!("{}: {stats:?}", path.display());
    Ok(())
}
