use kasane_animation::{sample_motion_curve, CompiledCurve};
use kasane_core::{Canvas, Document, Vec2};
fn decode_motion3(
    source: &str,
) -> Result<kasane_core::document::MotionClip, Box<dyn std::error::Error>> {
    let mut document = Document::new();
    assert!(document
        .initialize(
            "00000000-0000-4000-8000-000000000001",
            Canvas::new(100.0, 100.0, Vec2::default(), 10.0)
        )
        .is_ok());
    let id = "00000000-0000-4000-8000-000000000002";
    let imported = kasane_project::import_motion3(&document, id, "Sample", source).unwrap();
    Ok(imported.candidate.get_motion(id).unwrap().clone())
}

const RESTRICTED: &str =
    include_str!("../../../tests/fixtures/animation_cpu/bezier_restricted.motion3.json");
const UNRESTRICTED: &str =
    include_str!("../../../tests/fixtures/animation_cpu/bezier_unrestricted.motion3.json");
const TYPED: &str = include_str!("../../../tests/fixtures/animation_cpu/typed.motion3.json");

#[test]
fn beziers_match_official_framework_cpu_samples() {
    let cases = [
        (
            RESTRICTED,
            [
                0.0,
                0.223_437_5,
                0.325,
                0.351_562_5,
                0.35,
                0.367_187_5,
                0.45,
                0.645_312_5,
                1.0,
            ],
        ),
        (
            UNRESTRICTED,
            [
                0.000_000_710_705_25,
                0.257_220_95,
                0.333_164_3,
                0.351_470_44,
                0.350_415_3,
                0.354_238_87,
                0.389_742_5,
                0.509_106_2,
                1.0,
            ],
        ),
    ];
    for (source, expected) in cases {
        let motion = decode_motion3(source).unwrap();
        for (index, wanted) in expected.into_iter().enumerate() {
            let time = index as f32 * 0.125;
            let value = sample_motion_curve(
                &CompiledCurve::from(&motion.tracks[0]),
                time,
                motion.restricted_beziers,
            );
            assert!(
                (value - wanted).abs() < 0.00002,
                "restricted={} time={time}: {value} != {wanted}",
                motion.restricted_beziers
            );
        }
    }
}

#[test]
fn linear_stepped_and_inverse_stepped_use_framework_boundaries() {
    let motion = decode_motion3(TYPED).unwrap();
    let compiled = CompiledCurve::from(&motion.tracks[0]);
    let curve = &compiled;
    assert!((sample_motion_curve(curve, 0.125, false) - 0.5).abs() < 0.00001);
    assert!((sample_motion_curve(curve, 0.6, false) - 0.5).abs() < 0.00001);
    assert!((sample_motion_curve(curve, 0.75, false) - 0.9).abs() < 0.00001);
    assert!((sample_motion_curve(curve, 1.0, false) - 0.9).abs() < 0.00001);
}
