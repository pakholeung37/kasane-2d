use ag_psd::psd::{BlendMode, ColorMode, Layer, PixelData, Psd, WriteOptions};
use ag_psd::write_psd;
use kasane_core::image::decode_png;
use kasane_psd::import_psd;

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
