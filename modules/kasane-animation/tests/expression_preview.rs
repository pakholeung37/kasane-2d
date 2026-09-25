use kasane_animation::{AnimationError, ExpressionPreview};
use kasane_core::document::{ExpressionAsset, ExpressionBlend, ExpressionEntry, ExpressionTarget};
use kasane_core::{Canvas, Document, Parameter, Vec2};

const DOC: &str = "00000000-0000-4000-8000-000000000a01";
const X: &str = "00000000-0000-4000-8000-000000000a02";
const Y: &str = "00000000-0000-4000-8000-000000000a03";
const FIRST: &str = "00000000-0000-4000-8000-000000000a04";
const SECOND: &str = "00000000-0000-4000-8000-000000000a05";

fn document() -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    for (id, runtime_id) in [(X, "ParamX"), (Y, "ParamY")] {
        assert!(document
            .create_parameter(Parameter {
                id: id.into(),
                runtime_id: runtime_id.into(),
                name: runtime_id.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let first = ExpressionAsset {
        id: FIRST.into(),
        name: "First".into(),
        file_type: Some("Live2D Expression".into()),
        fade_in: Some(0.25),
        fade_out: Some(0.0),
        entries: vec![
            ExpressionEntry {
                target: ExpressionTarget::Resolved {
                    parameter_id: X.into(),
                },
                value: 0.75,
                blend: Some(ExpressionBlend::Overwrite),
                extensions: Default::default(),
            },
            ExpressionEntry {
                target: ExpressionTarget::Resolved {
                    parameter_id: Y.into(),
                },
                value: 0.5,
                blend: Some(ExpressionBlend::Multiply),
                extensions: Default::default(),
            },
        ],
        extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    assert!(document.create_expression(first).status.is_ok());
    let second = ExpressionAsset {
        id: SECOND.into(),
        name: "Second".into(),
        file_type: Some("Live2D Expression".into()),
        fade_in: None,
        fade_out: None,
        entries: vec![ExpressionEntry {
            target: ExpressionTarget::Resolved {
                parameter_id: X.into(),
            },
            value: 0.125,
            blend: None,
            extensions: Default::default(),
        }],
        extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    assert!(document.create_expression(second).status.is_ok());
    document
}

#[test]
fn matches_official_framework_expression_queue_trace() {
    let document = document();
    let revision = document.revision();
    let mut preview = ExpressionPreview::new(&document);
    preview.set_base_parameter(Y, 0.5).unwrap();
    preview.schedule_expression(FIRST, 0.0).unwrap();
    preview.schedule_expression(SECOND, 0.375).unwrap();
    // f32 representations of the official Framework probe values.
    let expected = [
        0.0,
        0.375,
        0.75,
        0.0,
        0.004_757_531,
        0.018_305_827,
        0.038_582_288,
    ];
    let expected_y = [
        0.5,
        0.375,
        0.25,
        0.25,
        0.259_515_05,
        0.286_611_68,
        0.327_164_6,
    ];
    for (index, wanted) in expected.into_iter().enumerate() {
        let dt = if index == 0 { 0.0 } else { 0.125 };
        let state = preview.advance(dt).unwrap();
        let value = state.parameters[X];
        assert!(
            (value - wanted).abs() < 0.00002,
            "frame {index}: {value} vs {wanted}"
        );
        let y = state.parameters[Y];
        assert!(
            (y - expected_y[index]).abs() < 0.00002,
            "frame {index}: {y} vs {}",
            expected_y[index]
        );
    }
    assert_eq!(document.revision(), revision);
    assert_eq!(preview.document_revision(), revision);
    let first_seek = preview.seek(0.75).unwrap().clone();
    let second_seek = preview.seek(0.75).unwrap().clone();
    assert_eq!(first_seek, second_seek);
}

#[test]
fn rejects_invalid_schedule_without_changing_state() {
    let mut preview = ExpressionPreview::new(&document());
    let before = preview.snapshot().clone();
    assert_eq!(
        preview.schedule_expression("missing", 0.0).unwrap_err(),
        AnimationError::MissingExpression("missing".into())
    );
    assert_eq!(
        preview.schedule_expression(FIRST, -1.0).unwrap_err(),
        AnimationError::InvalidTime
    );
    assert_eq!(preview.snapshot(), &before);
}

#[test]
fn standalone_and_combined_expression_stages_share_replay_semantics() {
    let doc = document();
    let mut standalone = ExpressionPreview::new(&doc);
    let mut combined = kasane_animation::MotionPreview::new(&doc);
    for (id, time) in [(FIRST, 0.0), (SECOND, 0.4), (FIRST, 0.8)] {
        standalone.schedule_expression(id, time).unwrap();
        combined.schedule_expression(id, time).unwrap();
    }
    for time in [0.0, 0.01, 1.25, 0.25, 0.8, 1.25] {
        assert_eq!(
            standalone.seek(time).unwrap().parameters,
            combined.seek(time).unwrap().parameters
        );
    }
    standalone.reset();
    combined.reset();
    for delta in [0.0, 0.2, 0.3, 0.4, 0.2] {
        assert_eq!(
            standalone.advance(delta).unwrap().parameters,
            combined.advance(delta).unwrap().parameters
        );
    }
}
