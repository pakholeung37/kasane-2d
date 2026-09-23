use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use kasane_core::{
    evaluation::{Drawable, DrawableFrame, OffscreenFrame, RenderCommand},
    types::{BlendMode, Canvas, Vec2},
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_wgpu::{
    WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig, WgpuTexture,
    WgpuTextureCatalog,
};

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

#[test]
fn flat_scene_renders_real_pixels() {
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("GPU adapter required for the WGPU acceptance test");
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("WGPU device");
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("source"),
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
        &[255, 0, 0, 255],
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
        "red".to_owned(),
        WgpuTexture {
            view: &source_view,
            width: 1,
            height: 1,
        },
    )]));
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("output"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
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
        canvas: Canvas::new(64.0, 64.0, Vec2::new(0.0, 0.0), 1.0),
        drawables: vec![Drawable {
            id: "red-triangle".into(),
            texture_asset_id: "red".into(),
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(63.0, 0.0),
                Vec2::new(0.0, -63.0),
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
    let config = WgpuTargetConfig {
        width: 64,
        height: 64,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut renderer = WgpuRenderer::new(&device, config).unwrap();
    renderer
        .render(
            &device,
            &queue,
            &output_view,
            &frame,
            &textures,
            ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64.0, 64.0),
                mask_scale: 1.0,
            },
        )
        .unwrap();
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 256 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap()
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let pixels = readback.get_mapped_range(..);
    let center = 8 * 256 + 8 * 4;
    assert_eq!(&pixels[center..center + 4], &[255, 0, 0, 255]);
    let outside = 60 * 256 + 60 * 4;
    assert_eq!(&pixels[outside..outside + 4], &[0, 0, 0, 0]);
}

fn quad(id: &str, texture: &str) -> Drawable {
    Drawable {
        id: id.into(),
        texture_asset_id: texture.into(),
        positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(64.0, 0.0),
            Vec2::new(64.0, -64.0),
            Vec2::new(0.0, -64.0),
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

fn render_fixture(frame: &DrawableFrame) -> (Vec<u8>, kasane_render_wgpu::WgpuRenderStats) {
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("GPU adapter required for the WGPU acceptance test");
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("WGPU device");
    let colors = [
        ("red", [255, 0, 0, 255]),
        ("green", [0, 255, 0, 255]),
        ("blue", [0, 0, 255, 255]),
        ("half", [255, 255, 255, 128]),
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
            .collect(),
    );
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("output"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
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
    let mut renderer = WgpuRenderer::new(
        &device,
        WgpuTargetConfig {
            width: 64,
            height: 64,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    )
    .unwrap();
    let mut scene_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("scene"),
    });
    let first = renderer
        .encode(
            WgpuEncodeTarget {
                device: &device,
                queue: &queue,
                encoder: &mut scene_encoder,
                output: &output_view,
                output_mode: WgpuOutputMode::Replace,
            },
            frame,
            &textures,
            ViewportConfig {
                transform: Affine2 {
                    origin: Vec2::new(1.0, 0.0),
                    ..Affine2::IDENTITY
                },
                target_extent: Vec2::new(64.0, 64.0),
                mask_scale: 1.0,
            },
        )
        .unwrap();
    assert!(first.vertex_upload_bytes > 0);
    queue.submit([scene_encoder.finish()]);
    let mut scene_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("scene-again"),
    });
    let stats = renderer
        .encode(
            WgpuEncodeTarget {
                device: &device,
                queue: &queue,
                encoder: &mut scene_encoder,
                output: &output_view,
                output_mode: WgpuOutputMode::Replace,
            },
            frame,
            &textures,
            ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64.0, 64.0),
                mask_scale: 1.0,
            },
        )
        .unwrap();
    assert_eq!(stats.vertex_upload_bytes, 0);
    assert_eq!(stats.index_upload_bytes, 0);
    assert_eq!(stats.geometry_buffer_creations, 0);
    queue.submit([scene_encoder.finish()]);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 256 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap()
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let pixels = readback.get_mapped_range(..).to_vec();
    (pixels, stats)
}

fn center(pixels: &[u8]) -> [u8; 4] {
    let offset = 32 * 256 + 32 * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}

#[test]
fn invisible_raw_mask_source_affects_visible_mesh() {
    let mut source = quad("source", "half");
    source.visible = false;
    source.opacity = 0.0;
    let mut target = quad("target", "red");
    target.masks = vec!["source".into()];
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![source, target],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.masks, 1);
    let pixel = center(&pixels);
    assert!(
        (i16::from(pixel[3]) - 128).abs() <= 2,
        "masked center: {pixel:?}"
    );
}

#[test]
fn offscreen_composites_child_into_parent() {
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("child", "red")],
        offscreens: vec![OffscreenFrame {
            id: "group".into(),
            enabled: true,
            opacity: 1.0,
            multiply_color: [1.0; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::BeginOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "child".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "group".into(),
            },
        ],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.active_surfaces, 1);
    assert_eq!(center(&pixels), [255, 0, 0, 255]);
}

#[test]
fn destination_read_works_without_host_copy_source() {
    let mut green = quad("green", "green");
    green.raw_blend_mode = Some(0);
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("red", "red"), green],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.destination_targets, 1);
    assert_eq!(center(&pixels), [0, 255, 0, 255]);
}

#[test]
fn consecutive_destination_reads_observe_the_latest_target() {
    let mut green = quad("green", "green");
    green.raw_blend_mode = Some(1);
    let mut blue = quad("blue", "blue");
    blue.raw_blend_mode = Some(1);
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("red", "red"), green, blue],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.destination_targets, 1);
    assert_eq!(center(&pixels), [255, 255, 255, 255]);
}

#[test]
fn nested_offscreens_render_before_parent_composite() {
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("inner-mesh", "red")],
        offscreens: vec![
            OffscreenFrame {
                id: "outer".into(),
                enabled: true,
                opacity: 1.0,
                multiply_color: [1.0; 4],
                ..Default::default()
            },
            OffscreenFrame {
                id: "inner".into(),
                parent_offscreen_id: Some("outer".into()),
                enabled: true,
                opacity: 1.0,
                multiply_color: [1.0; 4],
                ..Default::default()
            },
        ],
        render_plan: vec![
            RenderCommand::BeginOffscreen {
                offscreen_id: "outer".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "inner".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "inner-mesh".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "inner".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "outer".into(),
            },
        ],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.active_surfaces, 2);
    assert_eq!(center(&pixels), [255, 0, 0, 255]);
}

#[test]
fn offscreen_mask_uses_raw_source_alpha() {
    let mut source = quad("source", "half");
    source.visible = false;
    source.opacity = 0.0;
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![source, quad("child", "red")],
        offscreens: vec![OffscreenFrame {
            id: "group".into(),
            enabled: true,
            opacity: 1.0,
            masks: vec!["source".into()],
            multiply_color: [1.0; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "source".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "child".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "group".into(),
            },
        ],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.masks, 1);
    let pixel = center(&pixels);
    assert!(
        (i16::from(pixel[3]) - 128).abs() <= 2,
        "masked offscreen center: {pixel:?}"
    );
}

#[test]
fn fixed_additive_and_multiplicative_draws_match_expected_pixels() {
    for (mode, expected) in [
        (BlendMode::Additive, [255, 255, 0, 255]),
        (BlendMode::Multiplicative, [0, 0, 0, 255]),
    ] {
        let mut green = quad("green", "green");
        green.blend_mode = mode;
        let frame = DrawableFrame {
            canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
            drawables: vec![quad("red", "red"), green],
            ..Default::default()
        };
        let (pixels, _) = render_fixture(&frame);
        assert_eq!(center(&pixels), expected, "blend mode: {mode:?}");
    }
}

#[test]
fn inverted_opaque_mask_removes_the_draw() {
    let mut source = quad("source", "red");
    source.visible = false;
    let mut target = quad("target", "green");
    target.masks = vec!["source".into()];
    target.inverted_mask = true;
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![source, target],
        ..Default::default()
    };
    let (pixels, _) = render_fixture(&frame);
    assert_eq!(center(&pixels), [0, 0, 0, 0]);
}

#[test]
fn destination_read_on_offscreen_composite_sees_prior_parent_draw() {
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("background", "red"), quad("child", "green")],
        offscreens: vec![OffscreenFrame {
            id: "group".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: 1,
            multiply_color: [1.0; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "child".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "group".into(),
            },
        ],
        ..Default::default()
    };
    let (pixels, stats) = render_fixture(&frame);
    assert_eq!(stats.destination_targets, 1);
    assert_eq!(center(&pixels), [255, 255, 0, 255]);
}

#[test]
fn renderer_validates_format_and_rebuilds_for_resize() {
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("GPU adapter required for the WGPU acceptance test");
    let (device, _) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("WGPU device");
    let mut renderer = WgpuRenderer::new(
        &device,
        WgpuTargetConfig {
            width: 64,
            height: 64,
            format: wgpu::TextureFormat::Rgba8Unorm,
        },
    )
    .unwrap();
    renderer
        .resize(
            &device,
            WgpuTargetConfig {
                width: 32,
                height: 48,
                format: wgpu::TextureFormat::Bgra8Unorm,
            },
        )
        .unwrap();
    assert_eq!(renderer.target().width, 32);
    assert_eq!(renderer.target().height, 48);
    assert_eq!(renderer.target().format, wgpu::TextureFormat::Bgra8Unorm);
    assert_eq!(
        renderer
            .resize(
                &device,
                WgpuTargetConfig {
                    width: 32,
                    height: 48,
                    format: wgpu::TextureFormat::Rgba16Float,
                }
            )
            .unwrap_err()
            .code,
        "UNSUPPORTED_TARGET_FORMAT"
    );
    assert_eq!(renderer.target().format, wgpu::TextureFormat::Bgra8Unorm);
}
