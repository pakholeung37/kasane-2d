//! Synthetic storage benchmark; the deep-copy baseline is explicitly simulated.
use kasane_core::{
    document::{MotionPoint, MotionSegment},
    Canvas, Document, Vec2,
};
use std::{hint::black_box, sync::Arc, time::Instant};

fn main() {
    const TRACKS: usize = 64;
    const KEYS: usize = 2048;
    const ITERATIONS: usize = 100;
    let mut document = Document::new();
    assert!(document
        .initialize(
            "00000000-0000-4000-8000-000000000001",
            Canvas::new(100.0, 100.0, Vec2::default(), 10.0)
        )
        .is_ok());
    let id = "00000000-0000-4000-8000-000000000002";
    let imported = kasane_project::import_motion3(
        &document,
        id,
        "Benchmark",
        include_str!("../../../tests/fixtures/animation_cpu/loop.motion3.json"),
    )
    .unwrap();
    document = imported.candidate;
    let mut clip = document.get_motion(id).unwrap().clone();
    let template = clip.tracks[0].clone();
    clip.duration = KEYS as f32;
    clip.tracks = (0..TRACKS)
        .map(|index| {
            let mut track = template.clone();
            track.id = format!("00000000-0000-4000-8000-{:012x}", index + 100);
            track.segments = Arc::new(
                (1..=KEYS)
                    .map(|key| MotionSegment::Linear {
                        end: MotionPoint {
                            time: key as f32,
                            value: (key % 2) as f32,
                        },
                    })
                    .collect(),
            );
            track
        })
        .collect();
    assert!(document.replace_motion(clip).status.is_ok());
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(document.checkpoint());
    }
    let shared = start.elapsed();
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let mut clip = document.get_motion(id).unwrap().clone();
        for track in &mut clip.tracks {
            track.segments = Arc::new(track.segments.as_ref().clone());
        }
        black_box(clip);
    }
    let deep = start.elapsed();
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let mut clip = document.get_motion(id).unwrap().clone();
        Arc::make_mut(&mut clip.tracks[0].segments)[0] = MotionSegment::Linear {
            end: MotionPoint {
                time: 1.0,
                value: 0.25,
            },
        };
        black_box(clip);
    }
    let edit = start.elapsed();
    println!(
        "{}",
        serde_json::json!({
            "tracks": TRACKS, "segments_per_track": KEYS, "iterations": ITERATIONS,
            "checkpoint_ms": shared.as_secs_f64() * 1000.0,
            "simulated_deep_clip_clone_ms": deep.as_secs_f64() * 1000.0,
            "single_track_copy_on_write_ms": edit.as_secs_f64() * 1000.0
        })
    );
}
