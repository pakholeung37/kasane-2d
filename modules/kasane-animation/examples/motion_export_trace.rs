//! Export editor-authored track order and emit the corresponding CPU trace.
//! Used by run_framework_motion_wire_probe.py for cross-boundary validation.
use kasane_animation::MotionPreview;
use kasane_core::{Canvas, Document, Parameter, Part, Vec2};
use kasane_project::{export_motion3, import_motion3};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source = args.next().ok_or("source path required")?;
    let output = args.next().ok_or("output path required")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            "00000000-0000-4000-8000-000000000001",
            Canvas::new(100.0, 100.0, Vec2::default(), 10.0)
        )
        .is_ok());
    const X: &str = "00000000-0000-4000-8000-000000000002";
    const MOTION: &str = "00000000-0000-4000-8000-000000000005";
    for (id, runtime_id) in [
        (X, "ParamX"),
        ("00000000-0000-4000-8000-000000000003", "ParamY"),
    ] {
        assert!(doc
            .create_parameter(Parameter {
                id: id.into(),
                runtime_id: runtime_id.into(),
                name: runtime_id.into(),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                ..Default::default()
            })
            .status
            .is_ok());
    }
    assert!(doc
        .create_part(Part {
            id: "00000000-0000-4000-8000-000000000004".into(),
            runtime_id: "Part0".into(),
            name: "Part0".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let doc = import_motion3(&doc, MOTION, "Trace", &std::fs::read_to_string(source)?)?.candidate;
    std::fs::write(output, export_motion3(&doc, MOTION)?)?;
    let mut preview = MotionPreview::new(&doc);
    preview.schedule_motion(MOTION, 0.0)?;
    let mut samples = Vec::new();
    for index in 0..=8 {
        let snapshot = preview.advance(if index == 0 { 0.0 } else { 0.125 })?;
        samples.push(serde_json::json!({"time": snapshot.time, "param_x": snapshot.parameters[X]}));
    }
    println!("{}", serde_json::json!({"samples": samples}));
    Ok(())
}
