use std::collections::BTreeMap;

use kasane_core::document::{
    CdiCombinedSet, CdiParameterEntry, CdiParameterGroup, CdiParameterRef, DisplayInfo,
};
use kasane_core::preview::PreviewState;
use kasane_core::{Canvas, ChangeKind, Document, Parameter, Part, Vec2};
use serde_json::json;

const DOC: &str = "00000000-0000-4000-8000-000000000801";
const PARAM: &str = "00000000-0000-4000-8000-000000000802";
const PART: &str = "00000000-0000-4000-8000-000000000803";
const GROUP: &str = "00000000-0000-4000-8000-000000000804";
const CHILD: &str = "00000000-0000-4000-8000-000000000805";
const SET: &str = "00000000-0000-4000-8000-000000000806";

fn fixture_document() -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    assert!(document
        .create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamA".into(),
            name: "old".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(document
        .create_part(Part {
            id: PART.into(),
            runtime_id: "PartA".into(),
            name: "old part".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    document
}

fn info() -> DisplayInfo {
    DisplayInfo {
        parameters: Some(vec![CdiParameterEntry::Resolved {
            parameter_id: PARAM.into(),
            group_id: Some(GROUP.into()),
            extensions: BTreeMap::new(),
        }]),
        parameter_groups: Some(vec![
            CdiParameterGroup {
                id: GROUP.into(),
                runtime_id: "Face".into(),
                name: "顔".into(),
                parent_id: None,
                extensions: BTreeMap::new(),
            },
            CdiParameterGroup {
                id: CHILD.into(),
                runtime_id: "Child".into(),
                name: "子".into(),
                parent_id: Some(GROUP.into()),
                extensions: BTreeMap::new(),
            },
        ]),
        combined_parameters: Some(vec![CdiCombinedSet {
            id: SET.into(),
            members: vec![CdiParameterRef::Resolved {
                parameter_id: PARAM.into(),
            }],
        }]),
        ..Default::default()
    }
}

#[test]
fn cdi_identity_references_and_recovery_are_checked() {
    let mut uninitialized = Document::new();
    assert_eq!(
        uninitialized.replace_display_info(info()).status.code,
        "NOT_INITIALIZED"
    );
    let mut document = fixture_document();
    let mut invalid = info();
    invalid.parameter_groups.as_mut().unwrap()[0].id = PARAM.into();
    assert_eq!(
        document.replace_display_info(invalid).status.code,
        "DUPLICATE_ID"
    );
    let mut invalid = info();
    invalid.parameter_groups.as_mut().unwrap()[0].parent_id = Some(CHILD.into());
    assert_eq!(
        document.replace_display_info(invalid).status.code,
        "CDI_GROUP_CYCLE"
    );
    assert!(document.replace_display_info(info()).status.is_ok());
    assert!(document.contains_id(GROUP));
    assert!(document.contains_id(SET));
    assert_eq!(
        document.erase_object(GROUP).status.code,
        "OBJECT_REFERENCED"
    );
    assert_eq!(
        document.erase_object(PARAM).status.code,
        "OBJECT_REFERENCED"
    );
    assert_eq!(
        document.erase_object(SET).changes.kind,
        ChangeKind::Metadata
    );
    assert!(document
        .replace_display_info(DisplayInfo::default())
        .status
        .is_ok());
    assert!(document.erase_object(PARAM).status.is_ok());

    let mut document = fixture_document();
    let mut unresolved = DisplayInfo {
        parameters: Some(vec![CdiParameterEntry::Unresolved {
            runtime_id: "NewParam".into(),
            name: "unknown".into(),
            group_id: None,
            extensions: BTreeMap::new(),
        }]),
        ..DisplayInfo::default()
    };
    assert!(document
        .replace_display_info(unresolved.clone())
        .status
        .is_ok());
    let new_id = "00000000-0000-4000-8000-000000000807";
    assert_eq!(
        document
            .create_parameter(Parameter {
                id: new_id.into(),
                runtime_id: "NewParam".into(),
                ..Default::default()
            })
            .status
            .code,
        "DUPLICATE_RUNTIME_ID"
    );
    let mut candidate = document.fork_candidate();
    assert!(candidate
        .replace_display_info(DisplayInfo::default())
        .status
        .is_ok());
    assert!(candidate
        .create_parameter(Parameter {
            id: new_id.into(),
            runtime_id: "NewParam".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    unresolved.parameters = Some(vec![CdiParameterEntry::Resolved {
        parameter_id: new_id.into(),
        group_id: None,
        extensions: BTreeMap::new(),
    }]);
    assert!(candidate.replace_display_info(unresolved).status.is_ok());
    assert!(document
        .publish_candidate(candidate, ChangeKind::Structure, false)
        .unwrap());
    assert!(document.validate_structure().is_empty());
}

#[test]
fn display_metadata_preserves_evaluation_cache_through_checkpoint_exchange() {
    let mut document = fixture_document();
    document.mark_saved();
    let mut preview = PreviewState::default();
    let first = preview.frame(&document, 1).unwrap();
    let count = preview.evaluation_count();
    let evaluation = document.evaluation_revision();
    let mut old = document.checkpoint();
    assert!(document
        .set_parameter_display_name(PARAM, "新名称".into())
        .status
        .is_ok());
    assert!(document
        .set_part_display_name(PART, "部件".into())
        .status
        .is_ok());
    assert!(document.replace_display_info(info()).status.is_ok());
    assert_eq!(document.evaluation_revision(), evaluation);
    assert!(document.modified());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &preview.frame(&document, 1).unwrap()
    ));
    assert_eq!(preview.evaluation_count(), count);
    document.exchange_checkpoint(&mut old).unwrap();
    assert_eq!(document.evaluation_revision(), evaluation);
    assert!(!document.modified());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &preview.frame(&document, 1).unwrap()
    ));
    document.exchange_checkpoint(&mut old).unwrap();
    assert!(document.modified());
    assert_eq!(document.evaluation_revision(), evaluation);
    let old = document.checkpoint();
    let mut parameter = document.get_parameter(PARAM).unwrap().clone();
    parameter.runtime_id = "ParamRenamed".into();
    assert!(document.replace_parameter(parameter).status.is_ok());
    assert!(document.evaluation_revision() > evaluation);
    let mut old = old;
    let before = document.evaluation_revision();
    document.exchange_checkpoint(&mut old).unwrap();
    assert!(document.evaluation_revision() > before);
}

#[test]
fn nested_extension_maps_have_conservative_history_cost() {
    let mut document = fixture_document();
    let baseline = document.estimated_content_bytes();
    let mut info = DisplayInfo::default();
    info.extensions
        .insert("one".into(), json!({"nested":{"tiny":true}}));
    assert!(document.replace_display_info(info.clone()).status.is_ok());
    let one = document.estimated_content_bytes();
    assert!(
        one > baseline + 1000,
        "single-entry nested maps need node overhead"
    );
    info.extensions
        .insert("many".into(), json!({"a":1,"b":2,"c":3,"d":4}));
    assert!(document.replace_display_info(info).status.is_ok());
    assert!(document.estimated_content_bytes() > one + 1000);
    assert!(document.checkpoint().estimated_bytes() > baseline + 1000);
}
