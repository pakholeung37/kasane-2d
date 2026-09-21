use kasane_core::{preview::PreviewState, *};
use std::sync::Arc;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(id(1), Canvas::new(640., 480., Vec2::new(320., 240.), 100.))
        .is_ok());
    assert!(doc
        .create_parameter(Parameter {
            id: id(2),
            runtime_id: "P".into(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
        .status
        .is_ok());
    doc
}

#[test]
fn consumers_share_one_evaluation_and_snapshots_remain_immutable() {
    let doc = document();
    let mut state = PreviewState::default();
    let old = state.frame(&doc, 1).unwrap();
    for _ in 0..5 {
        assert!(Arc::ptr_eq(&old, &state.frame(&doc, 1).unwrap()));
    }
    assert_eq!(state.evaluation_count(), 1);
    assert!(state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap());
    let new = state.frame(&doc, 1).unwrap();
    assert_eq!(state.evaluation_count(), 2);
    assert_eq!(old.parameters[0].value, 0.);
    assert_eq!(new.parameters[0].value, 0.5);
    assert!(!state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap());
    assert_eq!(state.evaluation_count(), 2);
    // Distinct requests remain observable even when their clamped geometry matches.
    state.replace(&doc, 1, [(id(2), 10.)].into()).unwrap();
    let rev = state.revision();
    state.replace(&doc, 1, [(id(2), 20.)].into()).unwrap();
    assert_eq!(state.revision(), rev + 1);
    assert_eq!(state.frame(&doc, 1).unwrap().parameters[0].requested, 20.);
}

#[test]
fn rejected_requests_preserve_committed_state() {
    let doc = document();
    let mut state = PreviewState::default();
    state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap();
    let frame = state.frame(&doc, 1).unwrap();
    let revision = state.revision();
    for values in [[(id(2), f32::NAN)].into(), [(id(99), 1.)].into()] {
        assert!(state.replace(&doc, 1, values).is_err());
        assert_eq!(state.revision(), revision);
        assert_eq!(state.values()[&id(2)], 0.5);
        assert!(Arc::ptr_eq(&frame, &state.frame(&doc, 1).unwrap()));
    }
}

#[test]
fn document_changes_generation_restore_and_parameter_removal_invalidate() {
    let mut doc = document();
    let saved = doc.clone();
    let mut state = PreviewState::default();
    state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap();
    let first = state.frame(&doc, 1).unwrap();
    let mut p = doc.get_parameter(&id(2)).unwrap().clone();
    p.maximum = 0.25;
    assert!(doc.replace_parameter(p).status.is_ok());
    assert_eq!(state.frame(&doc, 1).unwrap().parameters[0].value, 0.25);
    doc.restore_from(&saved);
    assert_eq!(state.frame(&doc, 1).unwrap().parameters[0].value, 0.5);
    // Another document can have exactly the same revision.
    assert!(!Arc::ptr_eq(&first, &state.frame(&saved, 2).unwrap()));
    assert!(doc.erase_object(&id(2)).status.is_ok());
    state.retain_parameters(&doc);
    assert!(state.frame(&doc, 2).unwrap().parameters.is_empty());
    state.reset();
    assert!(state.values().is_empty());
}

#[test]
fn failed_current_frame_is_cached_without_mislabeling_an_old_frame() {
    let doc = Document::new();
    let mut state = PreviewState::default();
    for _ in 0..3 {
        assert_eq!(state.frame(&doc, 1).unwrap_err().code, "NOT_INITIALIZED");
    }
    assert_eq!(state.evaluation_count(), 1);
    let mut doc = doc;
    assert!(doc
        .initialize(id(1), Canvas::new(640., 480., Vec2::new(320., 240.), 100.))
        .is_ok());
    state.invalidate(); // Initialization intentionally does not increment Document revision.
    assert!(state.frame(&doc, 1).is_ok());
    assert_eq!(state.evaluation_count(), 2);
}

#[test]
fn late_geometry_failure_preserves_request_and_published_snapshot() {
    let mut doc = document();
    assert!(doc
        .add_asset(ImageAsset {
            id: id(3),
            source: "texture.png".into(),
            width: 1,
            height: 1,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_mesh(Mesh {
            id: id(4),
            texture_asset_id: id(3),
            vertex_ids: vec![1, 2, 3],
            base_positions: vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)],
            uvs: vec![Vec2::new(0., 0.); 3],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        })
        .status
        .is_ok());
    let saved = doc.clone();
    let mut state = PreviewState::default();
    state.replace(&doc, 1, [(id(2), 0.5)].into()).unwrap();
    let frame = state.frame(&doc, 1).unwrap();
    let revision = state.revision();
    let mut canvas = doc.canvas();
    canvas.pixels_per_unit = f32::MIN_POSITIVE;
    assert!(doc.replace_canvas(canvas).status.is_ok());
    assert_eq!(
        state
            .replace(&doc, 1, [(id(2), 0.75)].into())
            .unwrap_err()
            .code,
        "NON_FINITE"
    );
    assert_eq!(state.values()[&id(2)], 0.5);
    assert_eq!(state.revision(), revision);
    assert!(Arc::ptr_eq(&frame, &state.frame(&saved, 1).unwrap()));
    // The old successful frame must not be reported as current for the changed document.
    assert_eq!(state.frame(&doc, 1).unwrap_err().code, "NON_FINITE");
    doc.restore_from(&saved);
    assert_eq!(state.frame(&doc, 1).unwrap().drawables, frame.drawables);
}
