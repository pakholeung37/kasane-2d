//! Run with `cargo run --release -p kasane-sdk --example checkpoint_profile`.
//! Numbers are observations for this machine, not stable test thresholds.
use std::time::{Duration, Instant};

use kasane_core::{Canvas, ImageAsset, Vec2};
use kasane_sdk::{rectangle_mesh, AuthoringSession};

fn id(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn measure(meshes: u64) {
    let mut sdk = AuthoringSession::new(
        &id(1),
        Canvas::new(1000.0, 1000.0, Vec2::new(500.0, 500.0), 100.0),
    )
    .expect("valid document");
    sdk.edit("build", None, |edit| {
        edit.create_asset(ImageAsset {
            id: id(2),
            name: "synthetic".into(),
            source: "synthetic.png".into(),
            width: 2,
            height: 2,
            ..Default::default()
        })?;
        for index in 0..meshes {
            edit.create_mesh(rectangle_mesh(
                &id(index + 3),
                "quad",
                &id(2),
                Vec2::new(100.0, 100.0),
                Vec2::new(200.0, 200.0),
            )?)?;
        }
        Ok(())
    })
    .expect("valid fixture");

    let content_bytes = sdk.estimated_content_bytes();
    let iterations = 20;
    let mut total = Duration::ZERO;
    for _ in 0..iterations {
        let start = Instant::now();
        drop(sdk.begin_edit("probe", None).expect("candidate"));
        total += start.elapsed();
    }
    let start = Instant::now();
    let issues = sdk.validate_structure();
    let validation = start.elapsed();
    assert!(
        issues.is_empty(),
        "fixture has structural issues: {issues:?}"
    );
    let start = Instant::now();
    sdk.edit("rename", None, |edit| edit.rename_mesh(&id(3), "renamed"))
        .expect("metadata edit");
    let commit = start.elapsed();
    println!(
        "meshes={meshes} content_bytes={content_bytes} candidate_mean_us={} validate_us={} metadata_edit_us={} history_bytes={}",
        total.as_micros() / iterations,
        validation.as_micros(),
        commit.as_micros(),
        sdk.history_state().estimated_bytes,
    );
}

fn main() {
    measure(8);
    measure(512);
}
