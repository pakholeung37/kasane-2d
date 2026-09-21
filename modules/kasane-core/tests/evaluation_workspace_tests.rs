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
