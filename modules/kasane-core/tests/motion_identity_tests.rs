use kasane_core::document::{MotionClip, MotionEvent, MotionPoint, MotionTrack, MotionTrackTarget};
use kasane_core::{Canvas, Document, Parameter, Vec2};

const CLIP: &str = "00000000-0000-4000-8000-000000001001";
const TRACK: &str = "00000000-0000-4000-8000-000000001002";
const EVENT: &str = "00000000-0000-4000-8000-000000001003";

fn document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            "00000000-0000-4000-8000-000000001000",
            Canvas::new(100.0, 100.0, Vec2::default(), 10.0)
        )
        .is_ok());
    assert!(doc
        .create_motion(MotionClip {
            id: CLIP.into(),
            name: "Motion".into(),
            duration: 1.0,
            fps: 30.0,
            looping: false,
            restricted_beziers: true,
            fade_in: None,
            fade_out: None,
            tracks: vec![MotionTrack {
                id: TRACK.into(),
                target: MotionTrackTarget::Model {
                    runtime_id: "Opacity".into()
                },
                initial: MotionPoint {
                    time: 0.0,
                    value: 1.0
                },
                segments: Default::default(),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            }],
            events: vec![MotionEvent {
                id: EVENT.into(),
                time: 0.5,
                value: "tick".into(),
                extensions: Default::default()
            }],
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
        .status
        .is_ok());
    doc
}

#[test]
fn motion_children_reserve_ids_without_becoming_top_level_objects() {
    let mut doc = document();
    for id in [TRACK, EVENT] {
        assert!(doc.contains_id(id));
        let before = doc.clone();
        assert_eq!(
            doc.create_parameter(Parameter {
                id: id.into(),
                runtime_id: "Collision".into(),
                name: "Collision".into(),
                ..Default::default()
            })
            .status
            .code,
            "DUPLICATE_ID"
        );
        assert_eq!(doc.erase_object(id).status.code, "MISSING_OBJECT");
        assert_eq!(doc.revision(), before.revision());
        assert!(doc.same_content(&before));
        assert!(doc.validate_structure().is_empty());
    }
    // Replacing the owner is allowed to reuse its own nested IDs.
    let mut clip = doc.get_motion(CLIP).unwrap().clone();
    clip.tracks[0].initial.value = 0.5;
    clip.events[0].value = "changed".into();
    assert!(doc.replace_motion(clip.clone()).status.is_ok());
    clip.id = "00000000-0000-4000-8000-000000001004".into();
    clip.name = "Other".into();
    assert_eq!(doc.create_motion(clip).status.code, "DUPLICATE_ID");
    assert!(doc.erase_object(CLIP).status.is_ok());
    assert!(!doc.contains_id(TRACK));
    assert!(!doc.contains_id(EVENT));
    assert!(doc
        .create_parameter(Parameter {
            id: TRACK.into(),
            name: "Released".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc.validate_structure().is_empty());
}
