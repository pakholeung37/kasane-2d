use kasane_core::{document::ObjectLocks, Canvas, Part, Vec2};
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
            e.create_part(Part {
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
fn locks_are_atomic_canonical_metadata_with_history_and_cached_preview() {
    let mut s = session();
    let frame = s.preview_frame().unwrap();
    let evaluation = s.evaluation_revision();
    let before = s.version();
    let bytes = s.estimated_content_bytes();
    let locks = ObjectLocks {
        objects: vec![id(3), id(2)],
    };
    let (_, receipt) = s
        .edit("Lock", None, |e| e.replace_object_locks(locks.clone()))
        .unwrap();
    assert!(receipt.changed);
    assert_ne!(s.version(), before);
    assert_eq!(s.document().object_locks().objects, vec![id(2), id(3)]);
    assert!(s.estimated_content_bytes() > bytes);
    assert_eq!(s.evaluation_revision(), evaluation);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    let history = s.history_lengths();
    assert!(
        !s.edit("No-op", None, |e| e.replace_object_locks(locks.clone()))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.history_lengths(), history);
    s.undo().unwrap();
    assert!(s.document().object_locks().is_empty());
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    s.redo().unwrap();
    assert_eq!(s.evaluation_revision(), evaluation);
    assert!(Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    assert_eq!(s.preview_evaluation_count(), 1);
    // SDK authoring remains unrestricted and removes metadata with erased objects.
    s.edit("Erase locked object", None, |e| e.erase_object(&id(2)))
        .unwrap();
    assert_eq!(s.document().object_locks().objects, vec![id(3)]);
    s.undo().unwrap();
    assert_eq!(s.document().object_locks().objects, vec![id(2), id(3)]);
}
#[test]
fn invalid_lock_batch_aborts_without_publishing_or_discarding_redo() {
    let mut s = session();
    s.edit("Lock", None, |e| {
        e.replace_object_locks(ObjectLocks {
            objects: vec![id(2)],
        })
    })
    .unwrap();
    s.undo().unwrap();
    let before = s.version();
    let history = s.history_lengths();
    for objects in [vec![id(2), id(2)], vec![id(2), id(99)], vec![id(1)]] {
        assert!(s
            .edit("Invalid", None, |e| e
                .replace_object_locks(ObjectLocks { objects }))
            .is_err());
        assert_eq!(s.version(), before);
        assert_eq!(s.history_lengths(), history);
        assert!(s.document().object_locks().is_empty());
    }
}
#[test]
fn v7_roundtrip_old_versions_and_malformed_lock_metadata() {
    let mut s = session();
    s.edit("Lock", None, |e| {
        e.replace_object_locks(ObjectLocks {
            objects: vec![id(2)],
        })
    })
    .unwrap();
    let encoded = encode_project(s.document()).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], 7);
    let decoded = decode_project(&encoded).unwrap();
    assert_eq!(decoded.object_locks(), s.document().object_locks());
    for invalid in [
        serde_json::json!(null),
        serde_json::json!({"objects":[id(2),id(2)]}),
        serde_json::json!({"objects":[id(99)]}),
        serde_json::json!({"other":true}),
    ] {
        let mut bad = wire.clone();
        bad["document"]["object_locks"] = invalid;
        assert!(decode_project(&bad.to_string()).is_err());
    }
    for version in [5, 6] {
        let mut old = wire.clone();
        old["format_version"] = version.into();
        assert!(decode_project(&old.to_string()).is_err());
        old["document"]["object_locks"] = serde_json::json!({"objects":[]});
        assert!(decode_project(&old.to_string()).is_err());
        old["document"]
            .as_object_mut()
            .unwrap()
            .remove("object_locks");
        assert!(decode_project(&old.to_string())
            .unwrap()
            .object_locks()
            .is_empty());
    }
}
#[test]
fn save_reopen_and_dirty_baseline_include_locks() {
    let mut s = session();
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
    let dir =
        Temp(std::env::temp_dir().join(format!("kasane-locks-{}-{nonce}", std::process::id())));
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("locks.kasane");
    s.save_project(&path, None).unwrap();
    assert!(!s.document().modified());
    s.edit("Lock", None, |e| {
        e.replace_object_locks(ObjectLocks {
            objects: vec![id(2)],
        })
    })
    .unwrap();
    assert!(s.document().modified());
    let saved = s.save_project(&path, None).unwrap();
    assert!(!s.document().modified());
    s.undo().unwrap();
    assert!(s.document().modified());
    s.redo().unwrap();
    assert!(!s.document().modified());
    let mut opened = session();
    opened.open_project(&saved.manifest, None).unwrap();
    assert_eq!(opened.document().object_locks().objects, vec![id(2)]);
}
