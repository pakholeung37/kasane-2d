use ag_psd::psd::{BlendMode, ColorMode, Layer, PixelData, Psd, WriteOptions};
use ag_psd::write_psd;
use kasane_core::image::decode_png;
use kasane_psd::{import_psd, import_psd_pixels};

fn raster(name: &str, left: f64, top: f64, color: [u8; 4]) -> Layer {
    let mut layer = Layer::default();
    layer.additional_info.name = Some(name.into());
    layer.left = Some(left);
    layer.top = Some(top);
    layer.right = Some(left + 2.0);
    layer.bottom = Some(top + 2.0);
    layer.blend_mode = Some(BlendMode::Normal);
    layer.image_data = Some(PixelData {
        width: 2,
        height: 2,
        data: color.repeat(4),
    });
    layer
}

fn psd(layers: Vec<Layer>) -> Vec<u8> {
    let source = Psd {
        width: 8.0,
        height: 8.0,
        color_mode: Some(ColorMode::Rgb),
        bits_per_channel: Some(8.0),
        children: Some(layers),
        ..Psd::default()
    };
    write_psd(&source, &WriteOptions::default())
}

#[test]
fn shared_layered_fixture_is_generated_and_imported_here() {
    let fixture = include_bytes!("fixtures/layered.psd");
    let mut layer = raster("face", 1.0, 2.0, [255, 20, 30, 255]);
    layer.blend_mode = None;
    assert_eq!(fixture.as_slice(), psd(vec![layer]));
    let bundle = import_psd(fixture).unwrap();
    assert_eq!((bundle.report.width, bundle.report.height), (8, 8));
    assert_eq!(bundle.report.raster_layers, 1);
    assert_eq!(bundle.document.mesh_order().len(), 1);
}

#[test]
fn imports_cropped_layers_as_a_valid_document_and_pngs() {
    let bytes = psd(vec![
        raster("背面", 1.0, 2.0, [255, 0, 0, 128]),
        raster("前面", 3.0, 4.0, [0, 0, 255, 255]),
    ]);
    let bundle = import_psd(&bytes).unwrap();
    assert_eq!((bundle.report.width, bundle.report.height), (8, 8));
    assert_eq!(bundle.report.raster_layers, 2);
    assert_eq!(bundle.document.validate_structure().len(), 0);
    assert_eq!(bundle.document.mesh_order().len(), 2);
    assert_eq!(bundle.assets.len(), 2);
    assert_eq!(
        bundle
            .document
            .get_mesh(&bundle.document.mesh_order()[0])
            .unwrap()
            .name,
        "背面"
    );
    let front = bundle
        .document
        .get_mesh(&bundle.document.mesh_order()[1])
        .unwrap();
    assert_eq!(front.base_positions[0].x, 3.0);
    assert_eq!(front.base_positions[0].y, 4.0);
    assert_eq!(front.draw_order, Some(1.0));
    let decoded = decode_png(&bundle.assets[0].bytes).unwrap();
    assert_eq!((decoded.width, decoded.height), (2, 2));
    assert_eq!(&decoded.rgba[..4], &[255, 0, 0, 128]);
    assert_eq!(
        import_psd(&bytes).unwrap().assets[0].id,
        bundle.assets[0].id
    );
}

#[test]
fn pixel_import_preserves_png_pixels_ids_and_clipping_structure() {
    let base = raster("base", 1.0, 2.0, [255, 20, 30, 128]);
    let mut clipped = raster("clip", 3.0, 4.0, [0, 0, 255, 64]);
    clipped.clipping = Some(true);
    clipped.hidden = Some(true);
    clipped.blend_mode = Some(BlendMode::Multiply);
    let bytes = psd(vec![base, clipped]);
    let pngs = import_psd(&bytes).unwrap();
    let pixels = import_psd_pixels(&bytes).unwrap();
    assert_eq!(pixels.report, pngs.report);
    assert!(pixels.document.validate_structure().is_empty());
    assert_eq!(pixels.document.mesh_order(), pngs.document.mesh_order());
    for id in pixels.document.mesh_order() {
        assert_eq!(pixels.document.get_mesh(id), pngs.document.get_mesh(id));
    }
    assert_eq!(pixels.document.part_order(), pngs.document.part_order());
    for id in pixels.document.part_order() {
        assert_eq!(pixels.document.get_part(id), pngs.document.get_part(id));
    }
    assert_eq!(
        pixels.document.offscreen_order(),
        pngs.document.offscreen_order()
    );
    for id in pixels.document.offscreen_order() {
        assert_eq!(
            pixels.document.get_offscreen(id),
            pngs.document.get_offscreen(id)
        );
    }
    for (pixel, png) in pixels.assets.iter().zip(&pngs.assets) {
        assert_eq!(pixel.id, png.id);
        let decoded = decode_png(&png.bytes).unwrap();
        assert_eq!((pixel.width, pixel.height), (decoded.width, decoded.height));
        assert_eq!(pixel.rgba, decoded.rgba);
        assert!(pixels
            .document
            .get_asset(&pixel.id)
            .unwrap()
            .sha256
            .is_empty());
    }
}

#[test]
fn unique_identifier_layer_names_survive_as_runtime_ids() {
    let bundle = import_psd(&psd(vec![
        raster("ArtMesh15", 0.0, 0.0, [255, 0, 0, 255]),
        raster("ArtMesh15", 2.0, 0.0, [0, 255, 0, 255]),
        raster("目", 4.0, 0.0, [0, 0, 255, 255]),
    ]))
    .unwrap();
    let ids = bundle.document.mesh_order();
    let runtime_ids: Vec<_> = ids
        .iter()
        .map(|id| bundle.document.get_mesh(id).unwrap().runtime_id.as_str())
        .collect();
    assert_eq!(runtime_ids[0], "ArtMesh15");
    assert!(runtime_ids[1].starts_with("ArtMesh_"));
    assert!(runtime_ids[2].starts_with("ArtMesh_"));
    assert_eq!(
        runtime_ids.len(),
        runtime_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
}

#[test]
fn imports_groups_and_hidden_layers() {
    let mut group = Layer::default();
    group.additional_info.name = Some("头".into());
    let mut hidden = raster("眼", 0.0, 0.0, [0, 255, 0, 255]);
    hidden.hidden = Some(true);
    group.children = Some(vec![hidden]);
    let bundle = import_psd(&psd(vec![group])).unwrap();
    assert_eq!(bundle.report.groups, 1);
    let mesh = bundle
        .document
        .get_mesh(&bundle.document.mesh_order()[0])
        .unwrap();
    assert!(!mesh.enabled);
    assert_eq!(bundle.document.get_part(&mesh.part_id).unwrap().name, "头");
}

#[test]
fn nested_groups_keep_psd_stacking_between_raster_siblings() {
    let mut detail = Layer::default();
    detail.additional_info.name = Some("eyes".into());
    detail.children = Some(vec![raster("iris", 0.0, 0.0, [255; 4])]);
    let mut group = Layer::default();
    group.additional_info.name = Some("face".into());
    group.children = Some(vec![
        raster("skin", 0.0, 0.0, [255; 4]),
        detail,
        raster("hair", 0.0, 0.0, [255; 4]),
    ]);
    let bundle = import_psd(&psd(vec![
        raster("back", 0.0, 0.0, [255; 4]),
        group,
        raster("front", 0.0, 0.0, [255; 4]),
    ]))
    .unwrap();
    let mut frame = kasane_core::DrawableFrame::default();
    assert!(kasane_core::evaluate_frame(&bundle.document, &Default::default(), &mut frame).is_ok());
    frame
        .drawables
        .sort_by_key(|drawable| drawable.render_order);
    let rendered: Vec<_> = frame.drawables.iter().map(|d| d.id.clone()).collect();
    assert_eq!(rendered, bundle.document.mesh_order());
}

#[test]
fn clipping_layers_share_an_isolated_group_with_the_nearest_raster_base() {
    let base = raster("眼球底色", 0.0, 0.0, [255, 255, 255, 128]);
    let mut shadow = raster("眼球阴影", 1.0, 0.0, [0, 0, 0, 255]);
    shadow.clipping = Some(true);
    shadow.blend_mode = Some(BlendMode::Multiply);
    let mut highlight = raster("眼球高光", 0.0, 1.0, [255, 255, 255, 255]);
    highlight.clipping = Some(true);
    let outline = raster("眼球线", 0.0, 0.0, [0, 0, 0, 255]);
    let bundle = import_psd(&psd(vec![base, shadow, highlight, outline])).unwrap();
    let ids = bundle.document.mesh_order();
    assert!(bundle.document.get_mesh(&ids[0]).unwrap().masks.is_empty());
    let base = bundle.document.get_mesh(&ids[0]).unwrap();
    let offscreen = bundle.document.offscreen_for_part(&base.part_id).unwrap();
    assert_eq!(offscreen.blend_mode, 0);
    for (id, mode) in ids[1..3].iter().zip([262, 256]) {
        let clipped = bundle.document.get_mesh(id).unwrap();
        assert_eq!(clipped.part_id, base.part_id);
        assert_eq!(clipped.raw_blend_mode, Some(mode));
        assert!(clipped.masks.is_empty());
    }
    assert_ne!(
        bundle.document.get_mesh(&ids[3]).unwrap().part_id,
        base.part_id
    );
    assert!(bundle.document.get_mesh(&ids[3]).unwrap().masks.is_empty());
    assert!(bundle.document.validate_structure().is_empty());
    assert_eq!(decode_png(&bundle.assets[0].bytes).unwrap().rgba[3], 128);
}

#[test]
fn clipping_groups_preserve_raster_order_inside_nested_psd_groups() {
    let mut shadow = raster("shadow", 0.0, 0.0, [255; 4]);
    shadow.clipping = Some(true);
    let mut group = Layer::default();
    group.children = Some(vec![
        raster("base", 0.0, 0.0, [255; 4]),
        shadow.clone(),
        raster("next base", 0.0, 0.0, [255; 4]),
        shadow,
    ]);
    let bundle = import_psd(&psd(vec![
        raster("back", 0.0, 0.0, [255; 4]),
        group,
        raster("front", 0.0, 0.0, [255; 4]),
    ]))
    .unwrap();
    assert_eq!(bundle.report.groups, 1); // Synthetic clipping Parts aren't PSD groups.
    assert_eq!(bundle.document.offscreen_count(), 2);
    let mut frame = kasane_core::DrawableFrame::default();
    assert!(kasane_core::evaluate_frame(&bundle.document, &Default::default(), &mut frame).is_ok());
    let drawn: Vec<_> = frame
        .render_plan
        .iter()
        .filter_map(|command| match command {
            kasane_core::evaluation::RenderCommand::DrawMesh { mesh_id } => Some(mesh_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(drawn, bundle.document.mesh_order());
}

#[test]
fn rejects_clipping_blends_that_are_not_grouped() {
    let mut base = raster("base", 0.0, 0.0, [255; 4]);
    base.additional_info.blend_clippend_elements = Some(false);
    let mut clipped = raster("clipped", 0.0, 0.0, [255; 4]);
    clipped.clipping = Some(true);
    let error = import_psd(&psd(vec![base, clipped])).err().unwrap();
    assert_eq!(error.code, "UNSUPPORTED_LAYER");
    assert!(error.message.contains("Blend Clipped Layers As Group"));
}

#[test]
fn clipping_bases_do_not_leak_across_groups() {
    let mut clipped = raster("orphan", 0.0, 0.0, [0, 0, 0, 255]);
    clipped.clipping = Some(true);
    let mut group = Layer::default();
    group.children = Some(vec![clipped]);
    let error = import_psd(&psd(vec![raster("outside", 0.0, 0.0, [255; 4]), group]))
        .err()
        .unwrap();
    assert_eq!(error.code, "UNSUPPORTED_LAYER");
    assert!(error.message.contains("same group"));
}

#[test]
fn rejects_unsupported_layer_effect_without_partial_output() {
    let mut layer = raster("光效", 0.0, 0.0, [255, 255, 255, 255]);
    layer.blend_mode = Some(BlendMode::Overlay);
    let error = match import_psd(&psd(vec![layer])) {
        Ok(_) => panic!("unsupported blend must fail"),
        Err(error) => error,
    };
    assert_eq!(error.code, "UNSUPPORTED_LAYER");
}

#[test]
fn rejects_invalid_header_and_excessive_canvas_before_decode() {
    assert_eq!(import_psd(b"bad").err().unwrap().code, "INVALID_PSD");
    let mut bytes = psd(vec![raster("layer", 0.0, 0.0, [0, 0, 0, 255])]);
    bytes[18..22].copy_from_slice(&30_000u32.to_be_bytes());
    bytes[14..18].copy_from_slice(&30_000u32.to_be_bytes());
    assert_eq!(import_psd(&bytes).err().unwrap().code, "PSD_LIMIT");
}

#[test]
fn truncated_psd_does_not_panic() {
    let bytes = psd(vec![raster("layer", 0.0, 0.0, [0, 0, 0, 255])]);
    for length in 26..bytes.len() {
        assert!(
            std::panic::catch_unwind(|| import_psd(&bytes[..length])).is_ok(),
            "length {length}"
        );
    }
}

#[test]
fn imports_psd_created_by_the_existing_moc3_exporter() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/gpu/gpu-package/model.model3.json");
    let (bytes, exported) = kasane_moc3_psd::from_model3_file(&fixture).unwrap();
    let bundle = import_psd(&bytes).unwrap();
    assert_eq!(bundle.report.raster_layers, exported.layers);
    assert_eq!(bundle.assets.len(), exported.layers);
    assert!(bundle.document.validate_structure().is_empty());
}
