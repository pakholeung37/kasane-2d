use kasane_core::{history::History, *};
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn document(vertices: usize) -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(id(1), Canvas::new(640., 480., Vec2::new(320., 240.), 100.))
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            source: "texture.png".into(),
            width: 1,
            height: 1,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_mesh(Mesh {
            id: id(3),
            name: "original".into(),
            texture_asset_id: id(2),
            vertex_ids: (1..=vertices as u32).collect(),
            base_positions: (0..vertices)
                .map(|i| Vec2::new(i as f32, (i % 2) as f32))
                .collect(),
            uvs: vec![Vec2::default(); vertices],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        })
        .status
        .is_ok());
    doc.mark_saved();
    doc
}
fn rename(doc: &mut Document, history: &mut History, name: &str) {
    let edit = doc.rename_mesh(&id(3), name.into());
    assert!(edit.status.is_ok());
    history.record(doc, &edit);
}
fn vertex(doc: &mut Document, history: &mut History, x: f32) {
    let edit = doc.set_vertex_positions(&id(3), &[2], &[Vec2::new(x, 1.)]);
    assert!(edit.status.is_ok());
    history.record(doc, &edit);
}

#[test]
fn grouped_edits_merge_and_swap_without_losing_saved_semantics() {
    let mut doc = document(100);
    let mut h = History::default();
    assert!(h.begin(&doc, "drag and rename".into()).is_ok());
    for i in 1..100 {
        rename(&mut doc, &mut h, &format!("name{i}"));
        vertex(&mut doc, &mut h, i as f32);
    }
    assert!(h.estimated_bytes() < 1024);
    assert!(h.end(&doc).is_ok());
    assert_eq!(h.undo_len(), 1);
    let revision = doc.revision();
    assert!(h.undo(&mut doc).status.is_ok());
    assert_eq!(doc.revision(), revision + 1);
    assert!(!doc.modified());
    assert_eq!(doc.get_mesh(&id(3)).unwrap().base_positions[1].x, 1.);
    assert!(h.redo(&mut doc).status.is_ok());
    assert_eq!(doc.get_mesh(&id(3)).unwrap().name, "name99");
    assert_eq!(doc.get_mesh(&id(3)).unwrap().base_positions[1].x, 99.);
    doc.mark_saved();
    assert!(h.undo(&mut doc).status.is_ok());
    assert!(doc.modified());
    assert!(h.redo(&mut doc).status.is_ok());
    assert!(!doc.modified());
}

#[test]
fn cancel_and_noop_actions_preserve_redo_branch() {
    let mut doc = document(3);
    let mut h = History::default();
    rename(&mut doc, &mut h, "A");
    h.undo(&mut doc);
    assert!(h.begin(&doc, "cancel".into()).is_ok());
    rename(&mut doc, &mut h, "B");
    vertex(&mut doc, &mut h, 20.);
    assert!(h.cancel(&mut doc).status.is_ok());
    assert!(!doc.modified());
    assert_eq!(h.redo_len(), 1);
    h.begin(&doc, "no-op".into());
    rename(&mut doc, &mut h, "C");
    rename(&mut doc, &mut h, "original");
    assert!(h.end(&doc).is_ok());
    assert_eq!(h.undo_len(), 0);
    assert_eq!(h.redo_len(), 1);
    assert!(h.redo(&mut doc).status.is_ok());
    assert_eq!(doc.get_mesh(&id(3)).unwrap().name, "A");
    h.undo(&mut doc);
    rename(&mut doc, &mut h, "new branch");
    assert_eq!(h.redo_len(), 0);
}

#[test]
fn unsupported_and_unrecorded_edits_form_history_barriers() {
    let mut doc = document(3);
    let mut h = History::default();
    rename(&mut doc, &mut h, "A");
    h.begin(&doc, "unsupported".into());
    let edit = doc.replace_canvas(Canvas::new(100., 100., Vec2::default(), 100.));
    h.record(&mut doc, &edit);
    assert_eq!(h.notice(), Some("HISTORY_UNSUPPORTED_EDIT"));
    assert!(!h.active());
    assert_eq!(h.undo_len(), 0);
    assert!(!h.cancel(&mut doc).status.is_ok());
    rename(&mut doc, &mut h, "B");
    doc.rename_mesh(&id(3), "unrecorded".into());
    assert!(!h.undo(&mut doc).status.is_ok());
    assert_eq!(h.notice(), Some("HISTORY_EXTERNAL_EDIT"));
    assert_eq!(doc.get_mesh(&id(3)).unwrap().name, "unrecorded");
}

#[test]
fn limits_cover_pending_done_and_redo_data() {
    let mut doc = document(3);
    let mut h = History::with_limits(2, 1024);
    for name in ["A", "B", "C"] {
        rename(&mut doc, &mut h, name);
    }
    assert_eq!(h.undo_len(), 2);
    h.undo(&mut doc);
    h.undo(&mut doc);
    assert_eq!(doc.get_mesh(&id(3)).unwrap().name, "A");
    assert!(!h.undo(&mut doc).status.is_ok());
    assert!(h.estimated_bytes() <= 1024);
    // The before-value is retained, so replacing a large name is the oversized edit.
    rename(&mut doc, &mut h, &"x".repeat(2048));
    rename(&mut doc, &mut h, "small");
    assert_eq!(h.notice(), Some("HISTORY_LIMIT_EXCEEDED"));
    assert_eq!(h.undo_len(), 0);
    assert_eq!(h.redo_len(), 0);
    let mut h = History::with_limits(50, 100);
    h.begin(&doc, "budget".into());
    vertex(&mut doc, &mut h, 55.);
    assert!(!h.active());
    assert_eq!(h.notice(), Some("HISTORY_LIMIT_EXCEEDED"));
}

#[test]
fn failed_edits_and_vertex_transactions_are_atomic() {
    let mut doc = document(3);
    let mut h = History::default();
    h.begin(&doc, "batch".into());
    rename(&mut doc, &mut h, "A");
    let bad = doc.set_vertex_positions(&id(3), &[1, 99], &[Vec2::new(50., 0.), Vec2::new(2., 0.)]);
    assert!(!bad.status.is_ok());
    h.record(&mut doc, &bad);
    assert_eq!(doc.get_mesh(&id(3)).unwrap().base_positions[0].x, 0.);
    doc.begin_transaction();
    doc.stage_vertex_positions(VertexPositionUpdate {
        mesh_id: id(3),
        vertex_ids: vec![1],
        positions: vec![Vec2::new(50., 0.)],
    });
    assert!(!h.end(&doc).is_ok());
    assert!(!h.cancel(&mut doc).status.is_ok());
    let edit = doc.commit_transaction();
    h.record(&mut doc, &edit);
    h.end(&doc);
    assert!(h.undo(&mut doc).status.is_ok());
    assert!(!doc.modified());
}

#[test]
fn recorded_bytes_do_not_scale_with_mesh_size() {
    let mut small = document(3);
    let mut large = document(100_000);
    let mut a = History::default();
    let mut b = History::default();
    rename(&mut small, &mut a, "A");
    rename(&mut large, &mut b, "A");
    vertex(&mut small, &mut a, 2.);
    vertex(&mut large, &mut b, 2.);
    assert_eq!(a.estimated_bytes(), b.estimated_bytes());
    assert!(b.estimated_bytes() < 1024);
}
