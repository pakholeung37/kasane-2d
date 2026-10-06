#![cfg(target_os = "macos")]

use ag_psd::psd::{BlendMode, ColorMode, Layer, PixelData, Psd, WriteOptions};
use kasane_core::{image::decode_png, DrawableFrame, Vec2};
use kasane_psd::{import_psd, ImportBundle};
use kasane_render::{Affine2, ViewportConfig};
use kasane_render_metal::{
    metal::MTLPixelFormat, read_rgba8, MetalContext, MetalOutputMode, MetalRenderer,
    MetalTargetConfig, MetalTexture, MetalTextureCatalog,
};

fn raster(name: &str, rgba: [u8; 4]) -> Layer {
    let mut layer = Layer::default();
    layer.additional_info.name = Some(name.into());
    layer.left = Some(0.0);
    layer.top = Some(0.0);
    layer.right = Some(8.0);
    layer.bottom = Some(8.0);
    layer.blend_mode = Some(BlendMode::Normal);
    layer.image_data = Some(PixelData {
        width: 8,
        height: 8,
        data: rgba.repeat(64),
    });
    layer
}

fn import(layers: Vec<Layer>) -> ImportBundle {
    let psd = Psd {
        width: 8.0,
        height: 8.0,
        color_mode: Some(ColorMode::Rgb),
        bits_per_channel: Some(8.0),
        children: Some(layers),
        ..Default::default()
    };
    import_psd(&ag_psd::write_psd(&psd, &WriteOptions::default())).unwrap()
}

fn render(bundle: &ImportBundle) -> Vec<u8> {
    let mut frame = DrawableFrame::default();
    assert!(kasane_core::evaluate_frame(&bundle.document, &Default::default(), &mut frame).is_ok());
    let context = MetalContext::new().unwrap();
    let data: Vec<_> = bundle
        .assets
        .iter()
        .map(|asset| decode_png(&asset.bytes).unwrap())
        .collect();
    let textures: Vec<_> = data
        .iter()
        .map(|image| {
            context
                .upload_rgba8(image.width, image.height, &image.rgba, &[])
                .unwrap()
        })
        .collect();
    let catalog = MetalTextureCatalog::new(
        bundle
            .assets
            .iter()
            .zip(&textures)
            .zip(&data)
            .map(|((asset, texture), image)| {
                (
                    asset.id.clone(),
                    MetalTexture {
                        view: texture,
                        width: image.width,
                        height: image.height,
                    },
                )
            })
            .collect(),
    );
    let target = MetalTargetConfig {
        width: 8,
        height: 8,
        format: MTLPixelFormat::RGBA8Unorm,
    };
    let output = context.output_texture(target).unwrap();
    let mut renderer = MetalRenderer::new(&context, target).unwrap();
    renderer.sync_model(&frame, &catalog).unwrap();
    renderer
        .update_view(ViewportConfig {
            transform: Affine2::IDENTITY,
            target_extent: Vec2::new(8.0, 8.0),
            mask_scale: 1.0,
        })
        .unwrap();
    let command = context.queue().new_command_buffer();
    renderer
        .encode(command, &output, MetalOutputMode::Replace, &catalog)
        .unwrap();
    command.commit();
    command.wait_until_completed();
    read_rgba8(&output).unwrap()
}

fn assert_pixel(pixels: &[u8], x: usize, expected: [u8; 4]) {
    let actual = &pixels[(4 * 8 + x) * 4..][..4];
    for (actual_channel, expected_channel) in actual.iter().zip(expected) {
        assert!(
            (i16::from(*actual_channel) - i16::from(expected_channel)).abs() <= 2,
            "pixel at ({x}, 4): {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn clipping_preserves_base_opacity_pixel_alpha_and_visibility() {
    for (opacity, alpha, hidden, expected) in [
        (0.0, 255, false, [0, 0, 0, 0]),
        (0.5, 255, false, [0, 128, 0, 128]),
        (1.0, 128, false, [0, 128, 0, 128]),
        (0.5, 128, false, [0, 64, 0, 64]),
        (1.0, 255, true, [0, 0, 0, 0]),
    ] {
        let mut base = raster("base", [255, 0, 0, alpha]);
        base.opacity = Some(opacity);
        base.hidden = Some(hidden);
        let mut clipped = raster("clipped", [0, 255, 0, 255]);
        clipped.clipping = Some(true);
        assert_pixel(&render(&import(vec![base, clipped])), 4, expected);
    }
}

#[test]
fn editing_the_imported_base_updates_the_entire_clipping_group() {
    let mut clipped = raster("clipped", [0, 255, 0, 255]);
    clipped.clipping = Some(true);
    let mut bundle = import(vec![raster("base", [255, 0, 0, 255]), clipped]);
    let id = bundle.document.mesh_order()[0].clone();
    let mut base = bundle.document.get_mesh(&id).unwrap().clone();
    base.appearance.opacity = 0.5;
    assert!(bundle.document.replace_mesh(base.clone()).status.is_ok());
    assert_pixel(&render(&bundle), 4, [0, 128, 0, 128]);
    base.enabled = false;
    assert!(bundle.document.replace_mesh(base).status.is_ok());
    assert_pixel(&render(&bundle), 4, [0; 4]);
}

#[test]
fn consecutive_clipped_layers_blend_without_accumulating_alpha() {
    let mut green = raster("green", [0, 255, 0, 255]);
    green.clipping = Some(true);
    green.opacity = Some(0.5);
    let mut blue = raster("blue", [0, 0, 255, 255]);
    blue.clipping = Some(true);
    blue.opacity = Some(0.5);
    let bundle = import(vec![raster("base", [255, 0, 0, 128]), green, blue]);
    assert_pixel(&render(&bundle), 4, [32, 32, 64, 128]);
}

#[test]
fn clipping_is_isolated_from_background_and_neighboring_groups() {
    let mut left = raster("left base", [255, 0, 0, 128]);
    left.right = Some(4.0);
    left.image_data = Some(PixelData {
        width: 4,
        height: 8,
        data: [255, 0, 0, 128].repeat(32),
    });
    let mut green = raster("green", [0, 255, 0, 255]);
    green.clipping = Some(true);
    let mut right = left.clone();
    right.additional_info.name = Some("right base".into());
    right.left = Some(4.0);
    right.right = Some(8.0);
    right.image_data.as_mut().unwrap().data = [255, 0, 0, 0].repeat(32);
    let mut white = raster("white", [255; 4]);
    white.clipping = Some(true);
    let bundle = import(vec![
        raster("background", [0, 0, 255, 255]),
        left,
        green,
        right,
        white,
    ]);
    let pixels = render(&bundle);
    assert_pixel(&pixels, 2, [0, 128, 127, 255]);
    assert_pixel(&pixels, 6, [0, 0, 255, 255]);
}

#[test]
fn clipping_retains_supported_layer_and_group_color_blends() {
    for (mode, expected) in [
        (BlendMode::Normal, [25, 15, 0, 128]),
        (BlendMode::Multiply, [10, 1, 0, 128]),
        (BlendMode::LinearDodge, [75, 25, 0, 128]),
    ] {
        let mut clipped = raster("clipped", [50, 30, 0, 255]);
        clipped.clipping = Some(true);
        clipped.blend_mode = Some(mode);
        let bundle = import(vec![raster("base", [100, 20, 0, 128]), clipped]);
        assert_pixel(&render(&bundle), 4, expected);
    }
    for (mode, expected) in [
        (BlendMode::Multiply, [0, 78, 0, 255]),
        (BlendMode::LinearDodge, [100, 255, 100, 255]),
    ] {
        let mut base = raster("base", [255, 0, 0, 255]);
        base.blend_mode = Some(mode);
        let mut clipped = raster("clipped", [0, 200, 0, 255]);
        clipped.clipping = Some(true);
        let bundle = import(vec![
            raster("background", [100, 100, 100, 255]),
            base,
            clipped,
        ]);
        assert_pixel(&render(&bundle), 4, expected);
    }
}
