//! Independent, offscreen application host for real-model WGPU comparisons.
use std::error::Error;
use std::fs::{self, File};
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use kasane_render_wgpu::{WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig};
use kasane_wgpu_validate::{checked, Case, LoadedModel, TextureSet};
use serde_json::json;

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

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut encoder = png::Encoder::new(File::create(path)?, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

fn run(case_path: &Path, output_dir: &Path) -> Result<(), Box<dyn Error>> {
    let case = Case::read(case_path)?;
    fs::create_dir_all(output_dir)?;
    let model = LoadedModel::new(&case)?;
    let frame_summary = model.summary();
    let (view, scale, offset) = model.view(case.width, case.height, case.fit_long_side);

    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let adapter_info = adapter.get_info();
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let textures = TextureSet::upload(
        &device,
        &queue,
        &model,
        case.texture_profile == "linear_mipmap",
    )?;
    let catalog = textures.catalog();

    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("real-model.output"),
        size: wgpu::Extent3d {
            width: case.width,
            height: case.height,
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
    let mut renderer = checked(WgpuRenderer::new(
        &device,
        WgpuTargetConfig {
            width: case.width,
            height: case.height,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    ))?;
    checked(renderer.sync_model(&device, &model.frame, &catalog))?;
    checked(renderer.update_view(&device, view))?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("real-model.encode"),
    });
    let stats = checked(renderer.encode(
        WgpuEncodeTarget {
            device: &device,
            queue: &queue,
            encoder: &mut encoder,
            output: &output_view,
            output_mode: WgpuOutputMode::Replace,
        },
        &catalog,
    ))?;
    queue.submit([encoder.finish()]);

    let unpadded_row = case.width * 4;
    let padded_row = unpadded_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("real-model.readback"),
        size: u64::from(padded_row) * u64::from(case.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("real-model.copy"),
    });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(case.height),
            },
        },
        wgpu::Extent3d {
            width: case.width,
            height: case.height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(60)),
    })?;
    receiver.recv_timeout(Duration::from_secs(1))??;
    let mapped = readback.get_mapped_range(..);
    let mut pixels = Vec::with_capacity((unpadded_row as usize) * (case.height as usize));
    for row in mapped.chunks_exact(padded_row as usize) {
        pixels.extend_from_slice(&row[..unpadded_row as usize]);
    }
    let image_path = output_dir.join("wgpu.png");
    write_png(&image_path, case.width, case.height, &pixels)?;
    drop(mapped);
    readback.unmap();
    let report = json!({
        "status": "passed",
        "image": image_path,
        "frame_summary": frame_summary,
        "view": {"scale": scale, "offset": offset},
        "texture_profile": case.texture_profile,
        "mip_sha256": textures.mip_hashes,
        "texture_format": "Rgba8Unorm",
        "output_format": "Rgba8Unorm",
        "adapter": {"name": adapter_info.name, "backend": format!("{:?}", adapter_info.backend)},
        "render_stats": format!("{stats:?}"),
    });
    fs::write(
        output_dir.join("wgpu-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", image_path.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let case = args
        .next()
        .ok_or("usage: kasane-wgpu-validate CASE_JSON OUTPUT_DIR")?;
    let output = args
        .next()
        .ok_or("usage: kasane-wgpu-validate CASE_JSON OUTPUT_DIR")?;
    if args.next().is_some() {
        return Err("usage: kasane-wgpu-validate CASE_JSON OUTPUT_DIR".into());
    }
    run(Path::new(&case), Path::new(&output))
}
