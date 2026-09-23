use kasane_moc3_psd::{from_moc3_file, from_model3_file};
use std::collections::HashMap;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn u32be(bytes: &[u8], pos: usize) -> u32 {
    u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap())
}

#[test]
fn exports_real_model_with_layers_and_composite() {
    let (psd, report) = from_model3_file(&fixture("gpu/gpu-package/model.model3.json")).unwrap();
    assert_eq!((report.width, report.height, report.layers), (640, 480, 10));
    assert!(report.warnings.iter().any(|w| w.contains("masks")));
    assert_eq!(&psd[..4], b"8BPS");
    assert_eq!(u32be(&psd, 14), 480);
    assert_eq!(u32be(&psd, 18), 640);
    let layer_section = 26 + 4 + 4;
    let layer_info = layer_section + 4;
    assert!(u32be(&psd, layer_section) > 0);
    assert_eq!(
        i16::from_be_bytes(psd[layer_info + 4..layer_info + 6].try_into().unwrap()),
        10
    );
    assert!(psd.windows(4).any(|bytes| bytes == b"luni"));
    let composite = layer_section + 4 + u32be(&psd, layer_section) as usize;
    assert_eq!(&psd[composite..composite + 2], &[0, 0]);
    assert!(psd[composite + 2..].iter().any(|&value| value != 0));
}

#[test]
fn bare_moc3_needs_explicit_textures() {
    let moc3 = fixture("external_v50/model.moc3");
    let error = from_moc3_file(&moc3, &HashMap::new()).unwrap_err();
    assert!(error.to_string().contains("texture"));
    let texture = fixture("external_v50/texture_00.png");
    let (psd, report) = from_moc3_file(&moc3, &HashMap::from([(0, texture)])).unwrap();
    assert_eq!((report.width, report.height, report.layers), (400, 400, 1));
    assert_eq!(&psd[..4], b"8BPS");
}
