//! Deterministic, CPU-only equivalence trace. Serialization is outside timings.
//! Usage: cargo run --release -p cubism-matrix-kasane --example evaluation_trace -- MODEL3 OUTPUT
use kasane_core::{Canvas, Vec2};
use kasane_sdk::AuthoringSession;
use serde_json::json;
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let model = PathBuf::from(args.next().ok_or("missing MODEL3")?).canonicalize()?;
    let output = PathBuf::from(args.next().ok_or("missing OUTPUT")?);
    let mut session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000000001",
        Canvas::new(1., 1., Vec2::default(), 1.),
    )?;
    session.import_model3(&model, None)?;
    let mut preview = session.motion_preview();
    preview.schedule_motion_entry("Idle", 0, 0.)?;
    let mut writer = BufWriter::new(File::create(output)?);
    let (mut animation, mut geometry) = (0., 0.);
    for step in 0..600 {
        // Cover zero updates, irregular physics steps, loop boundaries, resets
        // and backwards/forwards canonical seeks without using wall-clock dt.
        if step == 200 {
            preview.seek(1.25)?;
        }
        if step == 400 {
            preview.reset();
        }
        let started = Instant::now();
        preview.advance([0., 1. / 60., 1. / 30., 0.05][step % 4])?;
        if preview.snapshot().active_motions.is_empty() {
            preview.schedule_motion_entry("Idle", 0, preview.snapshot().time)?;
        }
        animation += started.elapsed().as_secs_f64();
        let started = Instant::now();
        let frame = preview.evaluate_drawables()?;
        geometry += started.elapsed().as_secs_f64();
        let hidden = if step % 100 == 0 {
            Some(preview.evaluate_drawables_including_hidden()?)
        } else {
            None
        };
        serde_json::to_writer(
            &mut writer,
            &json!({"step":step, "snapshot":preview.snapshot(), "frame":frame, "hidden":hidden}),
        )?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    println!(
        "animation_ms={} geometry_ms={} frames=600",
        animation * 1000.,
        geometry * 1000.
    );
    Ok(())
}
