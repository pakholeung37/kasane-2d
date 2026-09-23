use kasane_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some(n) = count.get() {
                count.set(Some(n + 1));
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations(f: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(Some(0)));
    f();
    ALLOCATIONS.with(|count| count.replace(None).unwrap())
}

fn id(n: usize) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn document(count: usize) -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(id(1), Canvas::new(640., 480., Vec2::new(320., 240.), 100.))
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            source: "tex.png".into(),
            width: 64,
            height: 64,
            ..Default::default()
        })
        .status
        .is_ok());
    for n in 0..count {
        assert!(doc
            .create_mesh(Mesh {
                id: id(n + 3),
                texture_asset_id: id(2),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)],
                uvs: vec![Vec2::new(0., 0.); 3],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    doc
}

#[test]
fn saved_content_queries_do_not_allocate_and_still_detect_reverted_edits() {
    let mut doc = document(100);
    doc.mark_saved();
    let original = doc.clone();
    let count = allocations(|| {
        assert!(!doc.modified());
        assert!(doc.same_content(&original));
    });
    assert_eq!(count, 0);
    assert!(doc.rename_mesh(&id(3), "changed".into()).status.is_ok());
    assert!(doc.modified());
    assert!(!doc.same_content(&original));
    doc.restore_from(&original);
    assert!(!doc.modified());
}

#[test]
fn workspace_reuses_geometry_and_preserves_output_on_late_failure() {
    let mut doc = document(100);
    let preview = PreviewValues::new();
    let mut output = DrawableFrame::default();
    let mut evaluator = FrameEvaluator::default();
    for _ in 0..3 {
        assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    }
    let positions = output.drawables[0].positions.as_ptr();
    let reused = allocations(|| {
        assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    });
    assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    assert_eq!(positions, output.drawables[0].positions.as_ptr());
    let mut fresh = DrawableFrame::default();
    let one_shot = allocations(|| {
        assert!(evaluate_frame(&doc, &preview, &mut fresh).is_ok());
    });
    assert_eq!(output, fresh);
    // Callers own the output and may edit it; recycled slots must reset all fields.
    output.drawables[0].multiply_color[3] = 0.0;
    output.drawables[0].screen_color[3] = 0.0;
    for _ in 0..2 {
        assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    }
    assert_eq!(output, fresh);
    assert!(
        reused * 2 < one_shot,
        "reused={reused}, one_shot={one_shot}"
    );
    assert_eq!(
        reused, 0,
        "Warmed static meshes must not allocate per frame"
    );
    println!("100 meshes: reused={reused}, one_shot={one_shot} allocations");

    let valid_canvas = doc.canvas();
    let mut overflow_canvas = valid_canvas;
    overflow_canvas.pixels_per_unit = f32::MIN_POSITIVE;
    assert!(doc.replace_canvas(overflow_canvas).status.is_ok());
    assert_eq!(
        evaluator.evaluate(&doc, &preview, &mut output).code,
        "NON_FINITE"
    );
    assert_eq!(output, fresh);
    assert!(doc.replace_canvas(valid_canvas).status.is_ok());
    assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    assert!(evaluate_frame(&doc, &preview, &mut fresh).is_ok());
    assert_eq!(output, fresh);
}

// A renderer that retains the previous published frame through the next
// evaluation prevents PreviewState::invalidate from reclaiming its buffers.
// Measure that ownership cost before adopting persistent Arc frame caching.
#[test]
fn published_frame_ownership_preserves_snapshots_and_recycles_released_buffers() {
    use kasane_core::preview::PreviewState;

    let mut doc = document(100);
    let parameter = id(200);
    assert!(doc
        .create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.0,
            maximum: 1.0,
            ..Default::default()
        })
        .status
        .is_ok());

    let measure = |retain: bool| {
        let mut state = PreviewState::default();
        let mut retained: Option<std::sync::Arc<DrawableFrame>> = None;
        let mut step = |i| {
            let value = if i % 2 == 0 { -0.5 } else { 0.5 };
            assert!(state
                .replace(&doc, 1, [(parameter.clone(), value)].into())
                .unwrap());
            let frame = state.frame(&doc, 1).unwrap();
            assert_eq!(frame.drawables.len(), 100);
            assert_eq!(frame.parameters[0].value, value);
            if let Some(old) = &retained {
                assert_eq!(old.parameters[0].value, -value);
            }
            retained = retain.then_some(frame);
        };
        for i in 0..8 {
            step(i);
        }
        allocations(|| {
            for i in 8..20 {
                step(i);
            }
        })
    };
    let released = measure(false);
    let retained = measure(true);
    println!("12 publications / 100 meshes: released={released}, retained={retained} allocations");
    // Gate cheap recycling, but do not make the current retained-frame penalty
    // a required behavior: a future pool may legitimately improve that path.
    assert!(released < 120, "released={released}, retained={retained}");
}

#[test]
fn binding_index_tracks_retarget_delete_and_restore() {
    let mut doc = document(2);
    let parameter = id(10);
    assert!(doc
        .create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc.binding_for_mesh(&id(3)).is_none());
    let binding = MeshBinding {
        id: id(11),
        mesh_id: id(3),
        axes: vec![BindingAxis {
            parameter_id: parameter,
            keys: vec![0.],
        }],
        keyforms: vec![MeshKeyform {
            keys: vec![0.],
            positions: doc.get_mesh(&id(3)).unwrap().base_positions.clone(),
            ..Default::default()
        }],
    };
    assert!(doc.create_binding(binding.clone()).status.is_ok());
    assert_eq!(doc.binding_for_mesh(&id(3)).unwrap().id, binding.id);
    let saved = doc.clone();
    let mut moved = binding.clone();
    moved.mesh_id = id(4);
    assert!(doc.replace_binding(moved).status.is_ok());
    assert!(doc.binding_for_mesh(&id(3)).is_none());
    assert_eq!(doc.binding_for_mesh(&id(4)).unwrap().id, binding.id);
    assert!(doc.erase_object(&binding.id).status.is_ok());
    assert!(doc.binding_for_mesh(&id(4)).is_none());
    doc.restore_from(&saved);
    assert_eq!(doc.binding_for_mesh(&id(3)).unwrap().id, binding.id);
    assert!(doc.binding_for_mesh(&id(4)).is_none());
}

#[test]
fn static_geometry_survives_positions_and_names_but_not_topology_changes() {
    use std::sync::Arc;
    let mut doc = document(1);
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    let indices = frame.drawables[0].indices.clone();
    let uvs = frame.drawables[0].uvs.clone();
    assert!(doc.rename_mesh(&id(3), "renamed".into()).status.is_ok());
    assert!(doc
        .set_vertex_positions(&id(3), &[1], &[Vec2::new(2., 3.)])
        .status
        .is_ok());
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(Arc::ptr_eq(&indices, &frame.drawables[0].indices));
    assert!(Arc::ptr_eq(&uvs, &frame.drawables[0].uvs));
    let mut mesh = doc.get_mesh(&id(3)).unwrap().clone();
    mesh.uvs[0] = Vec2::new(0.5, 0.75);
    assert!(doc.replace_mesh(mesh).status.is_ok());
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(!Arc::ptr_eq(&uvs, &frame.drawables[0].uvs));
    assert_eq!(uvs[0], Vec2::new(0., 1.));
    // Reusing an evaluator on another document with the same revision is safe.
    let other = document(2);
    assert!(evaluator
        .evaluate(&other, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert_eq!(frame.drawables.len(), 2);
}

#[test]
fn metadata_reuses_preview_but_resources_and_restore_invalidate() {
    use kasane_core::preview::PreviewState;
    use std::sync::Arc;
    let mut doc = document(1);
    let saved = doc.clone();
    let mut preview = PreviewState::default();
    let first = preview.frame(&doc, 1).unwrap();
    assert!(doc.rename_mesh(&id(3), "renamed".into()).status.is_ok());
    assert!(Arc::ptr_eq(&first, &preview.frame(&doc, 1).unwrap()));
    assert_eq!(preview.evaluation_count(), 1);
    let mut asset = doc.get_asset(&id(2)).unwrap().clone();
    asset.source = "replacement.png".into();
    assert_eq!(doc.replace_asset(asset).changes.kind, ChangeKind::Resources);
    let second = preview.frame(&doc, 1).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(
        &first.drawables[0].uvs,
        &second.drawables[0].uvs
    ));
    doc.restore_from(&saved);
    assert!(!Arc::ptr_eq(&second, &preview.frame(&doc, 1).unwrap()));
}

#[test]
fn keyform_order_edit_invalidates_prepared_group_bounds() {
    let mut doc = document(2);
    assert!(doc
        .create_parameter(Parameter {
            id: id(10),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut form = MeshKeyform {
        keys: vec![0.],
        positions: doc.get_mesh(&id(3)).unwrap().base_positions.clone(),
        draw_order: Some(0.),
        ..Default::default()
    };
    assert!(doc
        .create_binding(MeshBinding {
            id: id(11),
            mesh_id: id(3),
            axes: vec![BindingAxis {
                parameter_id: id(10),
                keys: vec![0.]
            }],
            keyforms: vec![form.clone()],
        })
        .status
        .is_ok());
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.drawables[0].render_order < frame.drawables[1].render_order);
    form.draw_order = Some(10.);
    assert_eq!(
        doc.set_mesh_keyform(&id(11), form).changes.kind,
        ChangeKind::Structure
    );
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.drawables[0].render_order > frame.drawables[1].render_order);
}
