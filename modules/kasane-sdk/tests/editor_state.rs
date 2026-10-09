use kasane_core::{
    document::{EditorState, ObjectEditorState},
    Canvas, Part, Transform, Vec2,
};
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
        e.create_part(Part {
            id: id(2),
            enabled: false,
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(3),
            ..Default::default()
        })
    })
    .unwrap();
    s
}

#[test]
fn unified_flags_are_atomic_sparse_metadata_with_history_and_cleanup() {
    let mut s = session();
    let frame = s.preview_frame().unwrap();
    let mut state = EditorState::default();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: true,
        },
    );
    state.objects.insert(id(3), ObjectEditorState::default());
    s.edit("Hide and lock", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert_eq!(s.document().editor_state().objects.len(), 1);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let bytes = encode_project(s.document()).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(wire["format_version"], 10);
    assert!(wire["document"].get("object_locks").is_none());
    assert!(wire["document"].get("deformer_display").is_none());
    let loaded = decode_project(&bytes).unwrap();
    assert_eq!(loaded.editor_state(), s.document().editor_state());
    assert!(!loaded.get_part(&id(2)).unwrap().enabled);
    s.undo().unwrap();
    assert!(s.document().editor_state().is_empty());
    s.redo().unwrap();
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let history = s.history_lengths();
    let mut invalid = s.document().editor_state().clone();
    invalid.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    assert!(s
        .edit("Invalid", None, |e| e.replace_editor_state(invalid))
        .is_err());
    assert_eq!(s.history_lengths(), history);
    s.edit("Erase", None, |e| e.erase_object(&id(2))).unwrap();
    assert!(s.document().editor_state().is_empty());
    s.undo().unwrap();
    assert!(s.document().editor_state().get(&id(2)).locked);
}

#[test]
fn retired_fields_and_malformed_editor_state_are_rejected() {
    let s = session();
    let mut wire: serde_json::Value =
        serde_json::from_str(&encode_project(s.document()).unwrap()).unwrap();
    for (field, value) in [
        ("object_locks", serde_json::json!({"objects":[id(2)]})),
        ("deformer_display", serde_json::json!({"hidden":[id(3)]})),
    ] {
        wire["document"][field] = value;
        assert!(decode_project(&wire.to_string()).is_err());
        wire["document"].as_object_mut().unwrap().remove(field);
    }
    for invalid in [
        serde_json::json!(null),
        serde_json::json!({"objects":{id(99):{"locked":true}}}),
        serde_json::json!({"objects":{id(2):{"enabled":false}}}),
        serde_json::json!({"objects":{id(2):{"hidden_in_editor":"false"}}}),
    ] {
        wire["document"]["editor_state"] = invalid;
        assert!(decode_project(&wire.to_string()).is_err());
    }
    wire["document"]["editor_state"] = serde_json::json!({"objects":{id(2):{"locked":true}}});
    wire["format_version"] = 9.into();
    assert!(decode_project(&wire.to_string()).is_err());
}

#[test]
fn noops_and_invalid_batches_preserve_redo_and_runtime_cache() {
    let mut s = session();
    let frame = s.preview_frame().unwrap();
    let bytes = s.estimated_content_bytes();
    let mut state = EditorState::default();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: true,
        },
    );
    s.edit("Editor flags", None, |e| {
        e.replace_editor_state(state.clone())
    })
    .unwrap();
    assert!(s.estimated_content_bytes() > bytes);
    s.undo().unwrap();
    let history = s.history_lengths();
    let version = s.version();
    let mut invalid = state.clone();
    invalid.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    assert!(s
        .edit("Invalid batch", None, |e| e.replace_editor_state(invalid))
        .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    // Explicit defaults canonicalize to the same empty state and preserve redo.
    let defaults = EditorState {
        objects: [(id(2), ObjectEditorState::default())].into(),
    };
    assert!(
        !s.edit("No-op", None, |e| e.replace_editor_state(defaults))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.history_lengths(), history);
    s.redo().unwrap();
    let history = s.history_lengths();
    assert!(
        !s.edit("No-op", None, |e| e.replace_editor_state(state))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.history_lengths(), history);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
}

#[test]
fn saving_both_flags_roundtrips_and_establishes_history_clean_baseline() {
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = Temp(std::env::temp_dir().join(format!(
        "kasane-editor-state-{}-{nonce}",
        std::process::id()
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("state.kasane");
    let mut s = session();
    s.save_project(&path, None).unwrap();
    assert!(!s.document().modified());
    let state = EditorState {
        objects: [(
            id(3),
            ObjectEditorState {
                locked: true,
                hidden_in_editor: true,
            },
        )]
        .into(),
    };
    s.edit("Editor state", None, |e| {
        e.replace_editor_state(state.clone())
    })
    .unwrap();
    assert!(s.document().modified());
    let saved = s.save_project(&path, None).unwrap();
    assert!(!s.document().modified());
    s.undo().unwrap();
    assert!(s.document().modified());
    s.redo().unwrap();
    assert!(!s.document().modified());
    let mut reopened = session();
    reopened.open_project(&saved.manifest, None).unwrap();
    assert_eq!(reopened.document().editor_state(), &state);
}
