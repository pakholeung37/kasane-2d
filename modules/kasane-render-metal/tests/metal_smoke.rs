#![cfg(target_os = "macos")]

use std::{collections::HashMap, sync::Arc};

use kasane_core::{
    evaluation::{Drawable, DrawableFrame, OffscreenFrame, RenderCommand},
    types::{BlendMode, Canvas, Vec2},
};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal::MTLPixelFormat, read_rgba8, MetalContext, MetalOutputMode, MetalRenderStats,
    MetalRenderer, MetalTargetConfig, MetalTexture, MetalTextureCatalog,
};

#[test]
fn renders_pixels_through_native_metal() {
    const SIDE: u32 = 64;
    let context = MetalContext::new().unwrap();
    let source = context
        .upload_rgba8(1, 1, &[255, 40, 20, 255], &[])
        .unwrap();
    let textures = MetalTextureCatalog::new(HashMap::from([(
        "solid".to_owned(),
        MetalTexture {
            view: &source,
            width: 1,
            height: 1,
        },
    )]));
    let target = MetalTargetConfig {
        width: SIDE,
        height: SIDE,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
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
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &textures).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(SIDE as f32, SIDE as f32),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    renderer
        .encode(command, &output, MetalOutputMode::Replace, &textures)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    let pixels = read_rgba8(&output).unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * SIDE as usize + x) * 4..][..4];
    assert_eq!(pixel(8, 8), &[255, 40, 20, 255]);
    assert_eq!(pixel(60, 60), &[0, 0, 0, 0]);
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

#[test]
fn filtered_transparent_texels_do_not_tint_normal_or_clipped_edges() {
    let context = MetalContext::new().unwrap();
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let base = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    for transparent in [[0, 0, 0, 0], [255, 0, 255, 0]] {
        let mut rgba = vec![255, 255, 255, 255];
        rgba.extend(transparent);
        let source = context.upload_rgba8(2, 1, &rgba, &[]).unwrap();
        let catalog = MetalTextureCatalog::new(HashMap::from([
            (
                "edge".into(),
                MetalTexture {
                    view: &source,
                    width: 2,
                    height: 1,
                },
            ),
            (
                "base".into(),
                MetalTexture {
                    view: &base,
                    width: 1,
                    height: 1,
                },
            ),
        ]));
        for mode in [None, Some(0), Some(256)] {
            let mut edge = quad("edge", "edge");
            edge.raw_blend_mode = mode;
            let mut draws = vec![];
            if mode == Some(256) {
                draws.push(quad("base", "base"));
            }
            draws.push(edge);
            let frame = DrawableFrame {
                canvas: Canvas::new(64., 64., Vec2::new(0., 0.), 1.),
                drawables: draws,
                ..Default::default()
            };
            let output = context.output_texture(target).unwrap();
            let mut renderer = MetalRenderer::new(&context, target).unwrap();
            renderer.sync_model(&frame, &catalog).unwrap();
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2::IDENTITY,
                    target_extent: Vec2::new(64., 64.),
                    mask_scale: 1.,
                })
                .unwrap();
            let command = context.queue().new_command_buffer();
            renderer
                .encode(command, &output, MetalOutputMode::Replace, &catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            let pixels = read_rgba8(&output).unwrap();
            let actual = &pixels[(32 * 64 + 32) * 4..][..4];
            // At this UV, the opaque white texel contributes 31/64 coverage.
            let expected: [u8; 4] = if mode == Some(256) {
                [255, 124, 124, 255]
            } else {
                [124; 4]
            };
            for (a, e) in actual.iter().zip(expected) {
                assert!(
                    (i16::from(*a) - i16::from(e)).abs() <= 1,
                    "transparent={transparent:?}, mode={mode:?}: {actual:?}, expected={expected:?}"
                );
            }
        }
    }
}

fn fixture(frame: &DrawableFrame) -> (Vec<u8>, MetalRenderStats) {
    let context = MetalContext::new().unwrap();
    let colors = [
        ("red", [255, 0, 0, 255]),
        ("green", [0, 255, 0, 255]),
        ("blue", [0, 0, 255, 255]),
        ("half", [255, 255, 255, 128]),
    ];
    let sources: Vec<_> = colors
        .iter()
        .map(|(_, rgba)| context.upload_rgba8(1, 1, rgba, &[]).unwrap())
        .collect();
    let catalog = MetalTextureCatalog::new(
        colors
            .iter()
            .zip(&sources)
            .map(|((name, _), source)| {
                (
                    (*name).to_owned(),
                    MetalTexture {
                        view: source,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect(),
    );
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(64.0, 64.0),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    (read_rgba8(&output).unwrap(), stats)
}

fn center(pixels: &[u8]) -> [u8; 4] {
    pixels[32 * 256 + 32 * 4..][..4].try_into().unwrap()
}

#[test]
fn packed_mesh_buffers_preserve_vertex_and_index_offsets() {
    let mut left = quad("left", "red");
    for position in &mut left.positions {
        position.x *= 0.5;
    }
    // Only the upper/right triangle of the left half.
    left.indices = Arc::from([0, 1, 2]);
    let mut right = quad("right", "green");
    for position in &mut right.positions {
        position.x = 32. + position.x * 0.5;
    }
    let (pixels, stats) = fixture(&DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: vec![left, right],
        ..Default::default()
    });
    assert_eq!(stats.buffer_uploads, 2, "one vertex and one index upload");
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
    assert_eq!(pixel(24, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(8, 56), [0; 4]);
    assert_eq!(pixel(40, 56), [0, 255, 0, 255]);
    assert_eq!(pixel(56, 8), [0, 255, 0, 255]);
}

#[test]
fn raw_mask_and_inverted_mask_use_source_alpha() {
    let mut source = quad("source", "half");
    source.visible = false;
    source.opacity = 0.0;
    let mut target = quad("target", "red");
    target.masks = vec!["source".into()];
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![source, target],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.masks, 1);
    assert!((i16::from(center(&pixels)[3]) - 128).abs() <= 2);
    frame.drawables[1].inverted_mask = true;
    let (pixels, _) = fixture(&frame);
    assert!((i16::from(center(&pixels)[3]) - 127).abs() <= 2);
}

#[test]
fn duplicate_mask_sources_do_not_accumulate_alpha() {
    let mut source = quad("source", "half");
    source.visible = false;
    let mut target = quad("target", "red");
    target.masks = vec!["source".into()];
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![source, target],
        ..Default::default()
    };
    let expected = fixture(&frame).0;
    frame.drawables[1].masks.push("source".into());
    assert_eq!(center(&fixture(&frame).0), center(&expected));
}

#[test]
fn empty_mesh_does_not_allocate_a_destination_snapshot() {
    let mut empty = quad("empty", "red");
    empty.indices = Arc::from([]);
    empty.raw_blend_mode = Some(1);
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![empty],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.destination_copies, 0);
    assert_eq!(center(&pixels), [0; 4]);
}

#[test]
fn rejects_array_textures_before_encoding_or_readback() {
    use kasane_render_metal::metal::{
        MTLStorageMode, MTLTextureType, MTLTextureUsage, TextureDescriptor,
    };
    let context = MetalContext::new().unwrap();
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let descriptor = TextureDescriptor::new();
    descriptor.set_texture_type(MTLTextureType::D2Array);
    descriptor.set_array_length(2);
    descriptor.set_width(64);
    descriptor.set_height(64);
    descriptor.set_pixel_format(target.format);
    descriptor.set_storage_mode(MTLStorageMode::Shared);
    descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    let array = context.device().new_texture(&descriptor);
    let output = context.output_texture(target).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "array".into(),
        MetalTexture {
            view: &array,
            width: 64,
            height: 64,
        },
    )]));
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        ..Default::default()
    };
    let viewport = ViewportConfig {
        transform: Affine2::IDENTITY,
        target_extent: Vec2::new(64.0, 64.0),
        mask_scale: 1.0,
    };
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer.update_view(viewport).unwrap();
    let command = context.queue().new_command_buffer();
    assert_eq!(
        renderer
            .encode(command, &array, MetalOutputMode::Replace, &catalog)
            .unwrap_err()
            .code,
        "INVALID_TARGET"
    );
    frame.drawables.push(quad("mesh", "array"));
    renderer.sync_model(&frame, &catalog).unwrap();
    assert_eq!(
        renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap_err()
            .code,
        "INVALID_TEXTURE"
    );
    assert_eq!(read_rgba8(&array).unwrap_err().code, "INVALID_READBACK");
}

#[test]
fn nested_offscreens_composite_into_main() {
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
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.active_surfaces, 2);
    assert_eq!(center(&pixels), [255, 0, 0, 255]);
}

#[test]
fn zoomed_sibling_offscreens_reuse_attachments_and_preserve_draw_order() {
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: vec![quad("background", "red")],
        offscreens: vec![OffscreenFrame {
            id: "outer".into(),
            enabled: true,
            opacity: 0.5,
            multiply_color: [1.; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "outer".into(),
            },
        ],
        ..Default::default()
    };
    for i in 0..40 {
        let id = format!("group-{i}");
        let mut child = quad(&format!("child-{i}"), ["red", "green", "blue"][i % 3]);
        child.raw_blend_mode = Some(0);
        frame.offscreens.push(OffscreenFrame {
            id: id.clone(),
            parent_offscreen_id: Some("outer".into()),
            enabled: true,
            opacity: 0.5,
            blend_mode: 1,
            multiply_color: [1.; 4],
            ..Default::default()
        });
        frame.render_plan.extend([
            RenderCommand::BeginOffscreen {
                offscreen_id: id.clone(),
            },
            RenderCommand::DrawMesh {
                mesh_id: child.id.clone(),
            },
            RenderCommand::EndOffscreen { offscreen_id: id },
        ]);
        frame.drawables.push(child);
        // A parent draw between children catches lost Load actions and reordering.
        let mut parent = quad(&format!("parent-{i}"), "green");
        parent.opacity = 0.25;
        frame.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: parent.id.clone(),
        });
        frame.drawables.push(parent);
    }
    frame.render_plan.push(RenderCommand::EndOffscreen {
        offscreen_id: "outer".into(),
    });
    let expected = center(&fixture(&frame).0);
    let context = MetalContext::new().unwrap();
    let sources: Vec<_> = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .map(|rgba| context.upload_rgba8(1, 1, rgba, &[]).unwrap())
        .collect();
    let catalog = MetalTextureCatalog::new(
        ["red", "green", "blue"]
            .into_iter()
            .zip(&sources)
            .map(|(id, source)| {
                (
                    id.into(),
                    MetalTexture {
                        view: source,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect(),
    );
    let target = MetalTargetConfig {
        width: 2048,
        height: 2048,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(32., 0.),
                b: Vec2::new(0., 32.),
                origin: Vec2::default(),
            },
            target_extent: Vec2::new(2048., 2048.),
            mask_scale: 32.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    assert_eq!(stats.color_attachment_bytes, 3 * 2048 * 2048 * 4);
    command.commit();
    command.wait_until_completed();
    assert_eq!(stats.active_surfaces, 41);
    assert_eq!(stats.destination_copies, 80);
    let pixels = read_rgba8(&output).unwrap();
    for (x, y) in [(16, 16), (1024, 1024), (2032, 2032)] {
        assert_eq!(&pixels[(y * 2048 + x) * 4..][..4], &expected);
    }
}

#[test]
fn pooled_colors_preserve_parent_order_and_submitted_frames() {
    let context = MetalContext::new().unwrap();
    let sources: Vec<_> = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .map(|rgba| context.upload_rgba8(1, 1, rgba, &[]).unwrap())
        .collect();
    let catalog = MetalTextureCatalog::new(
        ["red", "green", "blue"]
            .into_iter()
            .zip(&sources)
            .map(|(id, source)| {
                (
                    id.into(),
                    MetalTexture {
                        view: source,
                        width: 1,
                        height: 1,
                    },
                )
            })
            .collect(),
    );
    let mut draws = vec![
        quad("background", "red"),
        quad("first", "blue"),
        quad("middle", "green"),
        quad("second", "red"),
        quad("last", "blue"),
    ];
    for i in [0, 1, 3] {
        draws[i].raw_blend_mode = Some(0);
    }
    draws[2].opacity = 0.5;
    draws[4].opacity = 0.5;
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: draws,
        offscreens: [
            ("outer", None, 0.5),
            ("first-group", Some("outer"), 1.),
            ("second-group", Some("outer"), 0.25),
        ]
        .into_iter()
        .map(|(id, parent, opacity)| OffscreenFrame {
            id: id.into(),
            parent_offscreen_id: parent.map(str::to_owned),
            enabled: true,
            opacity,
            multiply_color: [1.; 4],
            ..Default::default()
        })
        .collect(),
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "background".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "outer".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "first-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "first".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "first-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "middle".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "second-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "second".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "second-group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "last".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "outer".into(),
            },
        ],
        ..Default::default()
    };
    let target = MetalTargetConfig {
        width: 128,
        height: 96,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let view = ViewportConfig {
        transform: Affine2 {
            origin: Vec2::new(16.25, 8.75),
            ..Affine2::IDENTITY
        },
        target_extent: Vec2::new(128., 96.),
        mask_scale: 1.,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer.update_view(view).unwrap();
    let first_output = context.output_texture(target).unwrap();
    let first_command = context.queue().new_command_buffer();
    let first_stats = renderer
        .encode(
            first_command,
            &first_output,
            MetalOutputMode::Replace,
            &catalog,
        )
        .unwrap();
    assert_eq!(
        first_stats.color_attachment_bytes,
        (128 * 96 + 3 * 65 * 65) * 4
    );
    first_command.commit();
    // Submit a changed scene before waiting. Its pool must not overwrite the
    // previous command's attachments, even with a different presentation mode.
    for mesh in &mut frame.drawables {
        mesh.texture_asset_id = "red".into();
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer.update_view(view).unwrap();
    let second_output = context.output_texture(target).unwrap();
    let background = vec![255u8; 128 * 96 * 4];
    second_output.replace_region(
        kasane_render_metal::metal::MTLRegion::new_2d(0, 0, 128, 96),
        0,
        background.as_ptr().cast(),
        128 * 4,
    );
    let second_command = context.queue().new_command_buffer();
    let second_stats = renderer
        .encode(
            second_command,
            &second_output,
            MetalOutputMode::Composite,
            &catalog,
        )
        .unwrap();
    assert_eq!(
        second_stats.color_attachment_bytes,
        first_stats.color_attachment_bytes + 128 * 96 * 4
    );
    second_command.commit();
    second_command.wait_until_completed();
    let first = read_rgba8(&first_output).unwrap();
    let second = read_rgba8(&second_output).unwrap();
    let offset = (40 * 128 + 48) * 4;
    for (&actual, expected) in first[offset..][..4].iter().zip([144i16, 24, 88, 255]) {
        assert!(
            (i16::from(actual) - expected).abs() <= 2,
            "First frame pixel: {:?}",
            &first[offset..][..4]
        );
    }
    assert_eq!(&second[offset..][..4], &[255, 0, 0, 255]);
    assert_eq!(&first[..4], &[0; 4]);
    assert_eq!(
        &second[..4],
        &[255; 4],
        "Composite preserves the host background"
    );
}

#[test]
fn deeply_nested_offscreens_still_enforce_the_peak_attachment_budget() {
    let context = MetalContext::new().unwrap();
    let source = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &source,
            width: 1,
            height: 1,
        },
    )]));
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: vec![quad("mesh", "red")],
        ..Default::default()
    };
    for i in 0..9 {
        let id = format!("nested-{i}");
        frame.offscreens.push(OffscreenFrame {
            id: id.clone(),
            parent_offscreen_id: (i > 0).then(|| format!("nested-{}", i - 1)),
            enabled: true,
            opacity: 1.,
            multiply_color: [1.; 4],
            ..Default::default()
        });
        frame
            .render_plan
            .push(RenderCommand::BeginOffscreen { offscreen_id: id });
    }
    frame.render_plan.push(RenderCommand::DrawMesh {
        mesh_id: "mesh".into(),
    });
    frame
        .render_plan
        .extend((0..9).rev().map(|i| RenderCommand::EndOffscreen {
            offscreen_id: format!("nested-{i}"),
        }));
    let target = MetalTargetConfig {
        width: 4096,
        height: 4096,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(64., 0.),
                b: Vec2::new(0., 64.),
                origin: Vec2::default(),
            },
            target_extent: Vec2::new(4096., 4096.),
            mask_scale: 64.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let rejected = context.queue().new_command_buffer();
    for mode in [MetalOutputMode::Replace, MetalOutputMode::Composite] {
        assert_eq!(
            renderer
                .encode(rejected, &output, mode, &catalog)
                .unwrap_err()
                .code,
            "OFFSCREEN_BUDGET_EXCEEDED"
        );
    }
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        ..target
    };
    renderer.resize(target).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(64., 64.),
            mask_scale: 1.,
        })
        .unwrap();
    let output = context.output_texture(target).unwrap();
    let command = context.queue().new_command_buffer();
    let stats = renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    assert_eq!(
        stats.buffer_uploads, 2,
        "Budget rejection must preserve pending uploads"
    );
    command.commit();
    command.wait_until_completed();
    assert_eq!(center(&read_rgba8(&output).unwrap()), [255, 0, 0, 255]);
}

#[test]
fn fixed_and_destination_blends_see_prior_draws() {
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
        assert_eq!(center(&fixture(&frame).0), expected);
    }
    let mut green = quad("green", "green");
    green.raw_blend_mode = Some(1);
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("red", "red"), green],
        ..Default::default()
    };
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.destination_copies, 1);
    assert_eq!(center(&pixels), [255, 255, 0, 255]);
}

#[test]
fn offscreen_mask_and_destination_blend_use_parent_pixels() {
    let mut source = quad("source", "half");
    source.visible = false;
    source.opacity = 0.0;
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![source, quad("background", "red"), quad("child", "green")],
        offscreens: vec![OffscreenFrame {
            id: "group".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: 1,
            masks: vec!["source".into()],
            multiply_color: [1.0; 4],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "source".into(),
            },
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
    let (pixels, stats) = fixture(&frame);
    assert_eq!(stats.active_surfaces, 1);
    assert_eq!(stats.masks, 1);
    assert_eq!(stats.destination_copies, 1);
    let pixel = center(&pixels);
    assert_eq!(pixel[0], 255);
    assert!((i16::from(pixel[1]) - 128).abs() <= 2, "{pixel:?}");
    assert_eq!(pixel[2], 0);
    assert_eq!(pixel[3], 255);
}

#[test]
fn texture_rows_follow_the_shared_uv_orientation() {
    let context = MetalContext::new().unwrap();
    let mut rgba = vec![0u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let offset = (y * 64 + x) * 4;
            let color = match (x < 32, y < 32) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 255, 255],
            };
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }
    let source = context.upload_rgba8(64, 64, &rgba, &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "image".to_owned(),
        MetalTexture {
            view: &source,
            width: 64,
            height: 64,
        },
    )]));
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
    let frame = DrawableFrame {
        canvas: Canvas::new(64.0, 64.0, Vec2::default(), 1.0),
        drawables: vec![quad("mesh", "image")],
        ..Default::default()
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(64.0, 64.0),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    let pixels = read_rgba8(&output).unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
    assert_eq!(pixel(8, 8), &[0, 0, 255, 255]);
    assert_eq!(pixel(56, 8), &[255, 255, 255, 255]);
    assert_eq!(pixel(8, 56), &[255, 0, 0, 255]);
    assert_eq!(pixel(56, 56), &[0, 255, 0, 255]);
}

#[test]
fn view_changes_reuse_buffers_and_masks_but_source_changes_invalidate_them() {
    let context = MetalContext::new().unwrap();
    let source = context
        .upload_rgba8(1, 1, &[255, 255, 255, 128], &[])
        .unwrap();
    let color = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let mut catalog = MetalTextureCatalog::new(HashMap::from([
        (
            "half".into(),
            MetalTexture {
                view: &source,
                width: 1,
                height: 1,
            },
        ),
        (
            "red".into(),
            MetalTexture {
                view: &color,
                width: 1,
                height: 1,
            },
        ),
    ]));
    catalog.set_revision("half", 1);
    catalog.set_revision("red", 1);
    let mut mask = quad("mask", "half");
    mask.visible = false;
    let mut a = quad("a", "red");
    a.masks = vec!["mask".into()];
    let mut b = a.clone();
    b.id = "b".into();
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: vec![mask, a, b],
        ..Default::default()
    };
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let draw =
        |renderer: &mut MetalRenderer, catalog: &MetalTextureCatalog<'_>, pan: f32, scale: f64| {
            renderer
                .update_view(ViewportConfig {
                    transform: Affine2 {
                        origin: Vec2::new(pan, 0.),
                        ..Affine2::IDENTITY
                    },
                    target_extent: Vec2::new(64., 64.),
                    mask_scale: scale,
                })
                .unwrap();
            let output = context.output_texture(target).unwrap();
            let command = context.queue().new_command_buffer();
            let stats = renderer
                .encode(command, &output, MetalOutputMode::Replace, catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            assert_eq!(
                command.status(),
                kasane_render_metal::metal::MTLCommandBufferStatus::Completed
            );
            (read_rgba8(&output).unwrap(), stats)
        };
    renderer.sync_model(&frame, &catalog).unwrap();
    let (first, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!(stats.masks, 1, "consumers share the same raw mask");
    assert_eq!(stats.render_passes, 2, "one mask pass and one color pass");
    let (_, stats) = draw(&mut renderer, &catalog, 4., 1.);
    assert_eq!(
        (stats.masks, stats.buffer_uploads, stats.render_passes),
        (0, 0, 1)
    );
    assert_eq!(stats.mask_cache_hits, 2);
    // Re-submitting an unchanged model also preserves immutable geometry.
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 2.);
    assert_eq!(stats.masks, 1, "zoom changes mask density");
    assert_eq!(stats.buffer_uploads, 0);
    let (_, stats) = draw(&mut renderer, &catalog, 0., 1.1);
    assert_eq!(
        stats.masks, 0,
        "nearby zoom levels share rounded-up density"
    );
    // A source edit changes coverage and uploads only its vertex buffer.
    for p in &mut frame.drawables[0].positions {
        p.x += 64.;
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    let (moved, stats) = draw(&mut renderer, &catalog, 0., 2.);
    assert_eq!(stats.buffer_uploads, 1);
    assert_eq!(stats.masks, 1);
    assert_eq!(center(&moved), [0; 4]);
    for p in &mut frame.drawables[0].positions {
        p.x -= 64.;
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    let _ = draw(&mut renderer, &catalog, 0., 1.);
    // A new Arc with the same indices keeps the packed index buffer. A real
    // topology edit uploads just that buffer and invalidates dependent masks.
    frame.drawables[0].indices = Arc::from([0, 1, 2, 0, 2, 3]);
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!(stats.buffer_uploads, 0);
    frame.drawables[0].indices = Arc::from([0, 1, 2]);
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!((stats.buffer_uploads, stats.masks), (1, 1));
    frame.drawables[0].indices = Arc::from([0, 1, 2, 0, 2, 3]);
    renderer.sync_model(&frame, &catalog).unwrap();
    let _ = draw(&mut renderer, &catalog, 0., 1.);
    source.replace_region(
        kasane_render_metal::metal::MTLRegion::new_2d(0, 0, 1, 1),
        0,
        [255u8, 255, 255, 255].as_ptr().cast(),
        4,
    );
    catalog.set_revision("half", 2);
    let (opaque, stats) = draw(&mut renderer, &catalog, 0., 1.);
    assert_eq!(stats.masks, 1);
    assert_eq!(center(&opaque), [255, 0, 0, 255]);
    assert!(center(&first)[3] < 255);
    // Hosts without revision tracking must never get a stale cached mask.
    let unversioned = MetalTextureCatalog::new(HashMap::from([
        (
            "half".into(),
            MetalTexture {
                view: &source,
                width: 1,
                height: 1,
            },
        ),
        (
            "red".into(),
            MetalTexture {
                view: &color,
                width: 1,
                height: 1,
            },
        ),
    ]));
    for _ in 0..2 {
        assert_eq!(draw(&mut renderer, &unversioned, 0., 1.).1.masks, 1);
    }
    // Budget the shared allocation once, not once per consumer. The old
    // estimate exceeded 512 MiB here despite using a single ~4 MiB mask.
    for index in 0..200 {
        let mut consumer = frame.drawables[1].clone();
        consumer.id = format!("consumer-{index}");
        frame.drawables.push(consumer);
    }
    renderer.sync_model(&frame, &catalog).unwrap();
    let (_, stats) = draw(&mut renderer, &catalog, 0., 16.);
    assert_eq!(stats.masks, 1);
    assert_eq!(stats.mask_cache_hits, 201);
}

#[test]
fn edits_do_not_overwrite_buffers_referenced_by_submitted_frames() {
    let context = MetalContext::new().unwrap();
    let source = context.upload_rgba8(1, 1, &[255, 0, 0, 255], &[]).unwrap();
    let catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &source,
            width: 1,
            height: 1,
        },
    )]));
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        drawables: vec![quad("mesh", "red")],
        ..Default::default()
    };
    let mut outputs = Vec::new();
    for _ in 0..2 {
        renderer.sync_model(&frame, &catalog).unwrap();
        renderer
            .update_view(ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64., 64.),
                mask_scale: 1.,
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let command = context.queue().new_command_buffer();
        renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap();
        command.commit();
        outputs.push(output);
        for p in &mut frame.drawables[0].positions {
            p.x += 64.;
        }
    }
    let barrier = context.queue().new_command_buffer();
    barrier.commit();
    barrier.wait_until_completed();
    assert_eq!(center(&read_rgba8(&outputs[0]).unwrap()), [255, 0, 0, 255]);
    assert_eq!(center(&read_rgba8(&outputs[1]).unwrap()), [0; 4]);
}

#[test]
fn mask_atlas_matches_individual_masks_without_neighbor_bleed() {
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        ..Default::default()
    };
    for i in 0..40 {
        let mut source = quad(&format!("mask-{i}"), "half");
        let x = (i % 8) as f32 * 8. + 0.25;
        let y = (i / 8) as f32 * 12. + 0.125;
        for p in &mut source.positions {
            p.x = p.x / 16. + x;
            p.y = p.y / 8. - y;
        }
        source.visible = false;
        let mut consumer = source.clone();
        consumer.id = format!("consumer-{i}");
        consumer.texture_asset_id = "red".into();
        consumer.visible = true;
        consumer.masks = vec![source.id.clone()];
        consumer.inverted_mask = i % 2 == 0;
        // Cover both the mask edge and the transparent padding.
        consumer.positions[1].x += 3.;
        consumer.positions[2].x += 3.;
        frame.drawables.extend([source, consumer]);
    }
    let (atlas, stats) = fixture(&frame);
    assert_eq!(stats.masks, 40);
    assert_eq!(stats.render_passes, 2, "all masks share one atlas pass");
    let mut expected = vec![0u8; atlas.len()];
    for chunk in frame.drawables.chunks(40) {
        let (pixels, stats) = fixture(&DrawableFrame {
            canvas: frame.canvas,
            drawables: chunk.to_vec(),
            ..Default::default()
        });
        assert_eq!(stats.render_passes, 21);
        for (dst, src) in expected.iter_mut().zip(pixels) {
            *dst = (*dst).max(src); // These grid cells never overlap.
        }
    }
    assert!(atlas.iter().any(|&v| v != 0));
    assert_eq!(atlas, expected);
}

#[test]
fn rejected_encoding_preserves_mask_cache_and_pending_upload_stats() {
    for count in [1, 40] {
        let context = MetalContext::new().unwrap();
        let texture = context.upload_rgba8(1, 1, &[255, 0, 0, 128], &[]).unwrap();
        let mut catalog = MetalTextureCatalog::new(HashMap::from([(
            "red".into(),
            MetalTexture {
                view: &texture,
                width: 1,
                height: 1,
            },
        )]));
        catalog.set_revision("red", 1);
        let target = MetalTargetConfig {
            width: 64,
            height: 64,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalRenderer::new(&context, target).unwrap();
        let mut frame = DrawableFrame {
            canvas: Canvas::new(64., 64., Vec2::default(), 1.),
            ..Default::default()
        };
        for i in 0..count {
            let mut source = quad(&format!("mask-{i}"), "red");
            source.visible = false;
            let mut consumer = quad(&format!("consumer-{i}"), "red");
            consumer.masks = vec![source.id.clone()];
            frame.drawables.extend([source, consumer]);
        }
        renderer.sync_model(&frame, &catalog).unwrap();
        renderer
            .update_view(ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64., 64.),
                mask_scale: 1.,
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let invalid_output = context
            .output_texture(MetalTargetConfig {
                width: 32,
                ..target
            })
            .unwrap();
        let encode = |renderer: &mut MetalRenderer| {
            let command = context.queue().new_command_buffer();
            let stats = renderer
                .encode(command, &output, MetalOutputMode::Replace, &catalog)
                .unwrap();
            command.commit();
            command.wait_until_completed();
            stats
        };
        let command = context.queue().new_command_buffer();
        assert_eq!(
            renderer
                .encode(command, &invalid_output, MetalOutputMode::Replace, &catalog)
                .unwrap_err()
                .code,
            "INVALID_TARGET"
        );
        let stats = encode(&mut renderer);
        assert_eq!(stats.buffer_uploads, 2);
        assert_eq!(stats.masks, count);
        let expected = read_rgba8(&output).unwrap();

        let missing = MetalTextureCatalog::new(HashMap::new());
        let command = context.queue().new_command_buffer();
        assert_eq!(
            renderer
                .encode(command, &output, MetalOutputMode::Replace, &missing)
                .unwrap_err()
                .code,
            "MISSING_TEXTURE"
        );
        let stats = encode(&mut renderer);
        assert_eq!(stats.masks, 0);
        assert_eq!(stats.mask_cache_hits, count);
        assert_eq!(stats.buffer_uploads, 0);
        assert_eq!(read_rgba8(&output).unwrap(), expected);
    }
}

#[test]
fn atlas_cache_edits_and_transition_preserve_submitted_frames() {
    let context = MetalContext::new().unwrap();
    let texture = context.upload_rgba8(1, 1, &[255, 0, 0, 128], &[]).unwrap();
    let mut catalog = MetalTextureCatalog::new(HashMap::from([(
        "red".into(),
        MetalTexture {
            view: &texture,
            width: 1,
            height: 1,
        },
    )]));
    catalog.set_revision("red", 1);
    let target = MetalTargetConfig {
        width: 64,
        height: 64,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    let mut frame = DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::default(), 1.),
        ..Default::default()
    };
    for i in 0..40 {
        let mut source = quad(&format!("mask-{i}"), "red");
        source.visible = false;
        for p in &mut source.positions {
            p.x *= 0.5;
        }
        let mut consumer = quad(&format!("consumer-{i}"), "red");
        consumer.masks = vec![source.id.clone()];
        for p in &mut consumer.positions {
            p.x = p.x / 8. + (i % 8) as f32 * 8.;
            p.y = p.y / 8. - (i / 8) as f32 * 12.;
        }
        frame.drawables.extend([source, consumer]);
    }
    let mut outputs = Vec::new();
    for step in 0..5 {
        if step == 2 {
            for source in frame.drawables.iter_mut().step_by(2) {
                for p in &mut source.positions {
                    p.x += 32.;
                }
            }
        }
        if step == 3 {
            catalog.set_revision("red", 2);
        }
        if step == 4 {
            // Shrink below the atlas threshold and edit a remaining source.
            // Its cached atlas texture must not become a standalone mask.
            frame.drawables.truncate(40);
            for p in &mut frame.drawables[0].positions {
                p.x -= 32.;
            }
        }
        renderer.sync_model(&frame, &catalog).unwrap();
        renderer
            .update_view(ViewportConfig {
                transform: Affine2::IDENTITY,
                target_extent: Vec2::new(64., 64.),
                mask_scale: 1.,
            })
            .unwrap();
        let output = context.output_texture(target).unwrap();
        let command = context.queue().new_command_buffer();
        let stats = renderer
            .encode(command, &output, MetalOutputMode::Replace, &catalog)
            .unwrap();
        if step == 1 {
            assert_eq!(stats.masks, 0);
        }
        if step == 2 || step == 3 {
            assert_eq!(stats.masks, 40);
        }
        command.commit();
        outputs.push(output);
    }
    let barrier = context.queue().new_command_buffer();
    barrier.commit();
    barrier.wait_until_completed();
    let pixels: Vec<_> = outputs
        .iter()
        .map(|output| read_rgba8(output).unwrap())
        .collect();
    let pixel = |step: usize, x: usize| &pixels[step][(4 * 64 + x) * 4..][..4];
    assert_eq!(pixel(0, 4), [64, 0, 0, 64]);
    assert_eq!(pixel(0, 52), [0; 4]);
    assert_eq!(pixels[0], pixels[1]);
    assert_eq!(pixel(2, 4), [0; 4]);
    assert_eq!(pixel(2, 52), [64, 0, 0, 64]);
    assert_eq!(pixels[2], pixels[3]);
    assert_eq!(pixel(4, 4), [64, 0, 0, 64]);
}
