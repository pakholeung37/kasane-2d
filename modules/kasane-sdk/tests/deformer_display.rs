use kasane_core::{document::DeformerDisplay, Canvas, Transform, Vec2};
use kasane_project::{decode_project, encode_project};
use kasane_sdk::AuthoringSession;
use std::sync::Arc;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut s =
        AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    s.edit("Fixture", None, |e| {
        for n in [2, 3] {
            e.create_transform(Transform {
                id: id(n),
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();
    s
}

#[test]
fn control_visibility_is_atomic_metadata_with_history_and_unchanged_runtime_preview() {
    let mut s = session();
    let frame = s.preview_frame().unwrap();
    let revision = s.evaluation_revision();
    let bytes = s.estimated_content_bytes();
    let display = DeformerDisplay {
        hidden: vec![id(3), id(2)],
    };
    s.edit("Hide controls", None, |e| {
        e.replace_deformer_display(display.clone())
    })
    .unwrap();
    assert_eq!(s.document().deformer_display().hidden, vec![id(2), id(3)]);
    assert!(s.estimated_content_bytes() > bytes);
    assert_eq!(s.evaluation_revision(), revision);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let history = s.history_lengths();
    assert!(
        !s.edit("No-op", None, |e| e
            .replace_deformer_display(display.clone()))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.history_lengths(), history);
    s.undo().unwrap();
    assert!(s.document().deformer_display().is_empty());
    s.redo().unwrap();
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    assert!(s.document().get_transform(&id(2)).unwrap().enabled);
    s.edit("Erase", None, |e| e.erase_object(&id(2))).unwrap();
    assert_eq!(s.document().deformer_display().hidden, vec![id(3)]);
    s.undo().unwrap();
    assert_eq!(s.document().deformer_display().hidden, vec![id(2), id(3)]);
}

#[test]
fn v9_roundtrip_validates_controls_and_legacy_projects_default_to_visible() {
    let mut s = session();
    for hidden in [vec![id(2), id(2)], vec![id(99)], vec![id(1)]] {
        let version = s.version();
        assert!(s
            .edit("Invalid", None, |e| e
                .replace_deformer_display(DeformerDisplay { hidden }))
            .is_err());
        assert_eq!(s.version(), version);
        assert!(s.document().deformer_display().is_empty());
    }
    s.edit("Hide", None, |e| {
        e.replace_deformer_display(DeformerDisplay {
            hidden: vec![id(2)],
        })
    })
    .unwrap();
    let encoded = encode_project(s.document()).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], 9);
    assert_eq!(
        decode_project(&encoded).unwrap().deformer_display(),
        s.document().deformer_display()
    );
    for invalid in [
        serde_json::json!(null),
        serde_json::json!({"hidden":[id(99)]}),
        serde_json::json!({"hidden":[id(2),id(2)]}),
        serde_json::json!({"unknown":true}),
    ] {
        let mut bad = wire.clone();
        bad["document"]["deformer_display"] = invalid;
        assert!(decode_project(&bad.to_string()).is_err());
    }
    for version in [7, 8] {
        let mut old = wire.clone();
        old["format_version"] = version.into();
        assert!(decode_project(&old.to_string()).is_err());
        old["document"]
            .as_object_mut()
            .unwrap()
            .remove("deformer_display");
        assert!(decode_project(&old.to_string())
            .unwrap()
            .deformer_display()
            .is_empty());
    }
}
