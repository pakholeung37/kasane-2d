use kasane_core::{document::HierarchyOrder, Canvas, Part, Vec2};
use kasane_sdk::AuthoringSession;

#[test]
fn palette_metadata_keeps_evaluation_revision_and_survives_checkpoint_and_erasure(
) -> Result<(), Box<dyn std::error::Error>> {
    let first = "00000000-0000-4000-8000-000000000001".to_owned();
    let second = "00000000-0000-4000-8000-000000000002".to_owned();
    let mut session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000000003",
        Canvas::new(100., 100., Vec2::default(), 1.),
    )?;
    session.edit("Parts", None, |edit| {
        for id in [&first, &second] {
            edit.create_part(Part {
                id: id.clone(),
                ..Default::default()
            })?;
        }
        Ok(())
    })?;
    let evaluation = session.evaluation_revision();
    let order = HierarchyOrder {
        organization: vec![second.clone(), first.clone()],
        ..Default::default()
    };
    session.edit("Sort", None, |edit| {
        edit.replace_hierarchy_order(order.clone())
    })?;
    assert_eq!(session.evaluation_revision(), evaluation);
    session.undo()?;
    assert!(session.document().hierarchy_order().is_empty());
    assert_eq!(session.evaluation_revision(), evaluation);
    session.redo()?;
    assert_eq!(session.document().hierarchy_order(), &order);
    let invalid = HierarchyOrder {
        organization: vec![first.clone(), first.clone()],
        ..Default::default()
    };
    assert!(session
        .edit("Invalid", None, |edit| edit
            .replace_hierarchy_order(invalid))
        .is_err());
    assert_eq!(session.document().hierarchy_order(), &order);
    session.edit("Erase", None, |edit| edit.erase_object(&first))?;
    assert_eq!(
        session.document().hierarchy_order().organization,
        vec![second]
    );
    session.undo()?;
    assert_eq!(session.document().hierarchy_order(), &order);
    Ok(())
}

#[test]
fn project_codec_preserves_partial_order_and_rejects_invalid_references(
) -> Result<(), Box<dyn std::error::Error>> {
    use kasane_project::{decode_project, encode_project};
    let id = "00000000-0000-4000-8000-000000000001";
    let mut session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000000002",
        Canvas::new(100., 100., Vec2::default(), 1.),
    )?;
    session.edit("Fixture", None, |edit| {
        edit.create_part(Part {
            id: id.into(),
            ..Default::default()
        })?;
        edit.replace_hierarchy_order(HierarchyOrder {
            organization: vec![id.into()],
            ..Default::default()
        })
    })?;
    let encoded = encode_project(session.document()).unwrap();
    let decoded = decode_project(&encoded).unwrap();
    assert_eq!(
        decoded.hierarchy_order(),
        session.document().hierarchy_order()
    );
    let value: serde_json::Value = serde_json::from_str(&encoded)?;
    for invalid in [
        serde_json::json!({"organization": [id, id]}),
        serde_json::json!({"organization": ["00000000-0000-4000-8000-000000000099"]}),
        serde_json::json!({"deformation": [id]}),
    ] {
        let mut candidate = value.clone();
        candidate["document"]["hierarchy_order"] = invalid;
        assert!(decode_project(&candidate.to_string()).is_err());
    }
    let mut legacy = value;
    legacy["document"]
        .as_object_mut()
        .unwrap()
        .remove("hierarchy_order");
    assert!(decode_project(&legacy.to_string())
        .unwrap()
        .hierarchy_order()
        .is_empty());
    Ok(())
}
