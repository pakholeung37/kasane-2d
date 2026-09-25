use kasane_animation::MotionPreview;
use kasane_core::{Canvas, Document, ImageAsset, Mesh, Parameter, Part, Vec2};
use kasane_project::{import_expression3, import_motion3, import_physics3, import_pose3};

const DOC: &str = "00000000-0000-4000-8000-000000000d01";
const PARAM: &str = "00000000-0000-4000-8000-000000000d02";
const MOTION: &str = "00000000-0000-4000-8000-000000000d03";

fn document() -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    assert!(document
        .create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    document
}

#[test]
fn physics_inputs_replay_with_seek_without_editing_document() {
    let mut doc = document();
    let y = "00000000-0000-4000-8000-000000000d0a";
    assert!(doc
        .create_parameter(Parameter {
            id: y.into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let physics = import_physics3(
        &doc,
        "00000000-0000-4000-8000-000000000d0b",
        include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json"),
    )
    .unwrap()
    .candidate;
    let revision = physics.revision();
    let mut preview = MotionPreview::new(&physics);
    let mut time = 0.0f32;
    for (dt, input) in [
        (1.0 / 60.0, 0.0),
        (1.0 / 60.0, 1.0),
        (1.0 / 60.0, 1.0),
        (1.0 / 60.0, -1.0),
        (1.0 / 30.0, -1.0),
        (0.1, 0.0),
    ] {
        time += dt;
        preview
            .schedule_parameter_input(PARAM, time, input)
            .unwrap();
    }
    let first = preview.seek(time).unwrap().parameters[y];
    let second = preview.seek(time).unwrap().parameters[y];
    assert_eq!(first, second);
    assert_eq!(physics.revision(), revision);
}

#[test]
fn expression_then_physics_replays_in_one_preview() {
    let mut doc = document();
    let y = "00000000-0000-4000-8000-000000000d0c";
    assert!(doc
        .create_parameter(Parameter {
            id: y.into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let expression_id = "00000000-0000-4000-8000-000000000d0d";
    let source = r#"{"Type":"Live2D Expression","FadeInTime":0,"Parameters":[{"Id":"ParamX","Value":0.75,"Blend":"Add"}]}"#;
    let doc = import_expression3(&doc, expression_id, "Add", source)
        .unwrap()
        .candidate;
    let doc = import_physics3(
        &doc,
        "00000000-0000-4000-8000-000000000d0e",
        include_str!("../../../tests/fixtures/animation_cpu/missing.physics3.json"),
    )
    .unwrap()
    .candidate;
    let mut preview = MotionPreview::new(&doc);
    preview.schedule_expression(expression_id, 0.0).unwrap();
    assert_eq!(preview.advance(1.0 / 60.0).unwrap().parameters[PARAM], 0.75);
    preview.advance(1.0 / 60.0).unwrap();
    let third = preview.advance(1.0 / 60.0).unwrap().clone();
    assert_eq!(third.active_expressions, vec![expression_id.to_string()]);
    assert!(third.parameters[y].abs() > 0.01);
    let first_seek = preview.seek(0.05).unwrap().parameters[y];
    assert_eq!(preview.seek(0.05).unwrap().parameters[y], first_seek);
}

#[test]
fn loop_v2_matches_official_framework_frames() {
    let source = include_str!("../../../tests/fixtures/animation_cpu/loop.motion3.json");
    let imported = import_motion3(&document(), MOTION, "Loop", source).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    for (dt, expected) in [
        (0.25, 0.0),
        (0.25, 0.25),
        (0.5, 0.75),
        (0.25, 1.0),
        (0.25, 0.5),
        (0.25, 1.0),
        (0.5, 0.5),
    ] {
        let state = preview.advance(dt).unwrap();
        assert!(
            (state.parameters[PARAM] - expected).abs() < 0.00002,
            "time={} actual={} expected={expected}",
            state.time,
            state.parameters[PARAM]
        );
    }
    let first_seek = preview.seek(2.25).unwrap().parameters[PARAM];
    assert_eq!(preview.seek(2.25).unwrap().parameters[PARAM], first_seek);
    let before_cancel = preview.snapshot().clone();
    let mut updates = Vec::new();
    assert_eq!(
        preview
            .seek_with_progress(1.0, |done, total| {
                updates.push((done, total));
                done < 3
            })
            .unwrap_err(),
        kasane_animation::AnimationError::SeekCancelled
    );
    assert_eq!(preview.snapshot(), &before_cancel);
    assert_eq!(updates[0], (0, 60));
    assert_eq!(
        preview.seek_with_progress(1.0, |_, _| true).unwrap().time,
        1.0
    );
    assert!(imported.candidate.get_motion(MOTION).is_some());
}

#[test]
fn event_cursor_and_unresolved_target_guard_are_explicit() {
    let typed = include_str!("../../../tests/fixtures/animation_cpu/typed.motion3.json");
    let imported = import_motion3(&document(), MOTION, "Typed", typed).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    preview.advance(0.0).unwrap();
    assert!(preview.advance(0.5).unwrap().fired_events.is_empty());
    assert_eq!(preview.advance(0.25).unwrap().fired_events[0].value, "你好");
    assert!(preview.advance(0.25).unwrap().fired_events.is_empty());
    let unresolved = include_str!("../../../tests/fixtures/animation_cpu/minimal.motion3.json");
    let imported = import_motion3(
        &document(),
        "00000000-0000-4000-8000-000000000d04",
        "Missing",
        unresolved,
    )
    .unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    assert!(preview
        .schedule_motion("00000000-0000-4000-8000-000000000d04", 0.0)
        .is_err());
}

#[test]
fn pose_virtual_controls_follow_official_motion_and_pose_frames() {
    let mut doc = document();
    assert!(doc
        .create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000d05".into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let part0 = "00000000-0000-4000-8000-000000000d06";
    let part1 = "00000000-0000-4000-8000-000000000d07";
    for (id, runtime) in [(part0, "Part0"), (part1, "Part1")] {
        assert!(doc
            .create_part(Part {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let asset = "00000000-0000-4000-8000-000000000d09";
    assert!(doc
        .add_asset(ImageAsset {
            id: asset.into(),
            source: "memory://test".into(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());
    for (index, part_id) in [part0, part1].iter().enumerate() {
        assert!(doc
            .create_mesh(Mesh {
                id: format!("00000000-0000-4000-8000-000000000d1{index}"),
                texture_asset_id: asset.into(),
                part_id: (*part_id).into(),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0)
                ],
                uvs: vec![Vec2::new(0.0, 0.0); 3],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let motion = include_str!("../../../tests/fixtures/animation_cpu/minimal.motion3.json");
    let pose = include_str!("../../../tests/fixtures/animation_cpu/minimal.pose3.json");
    let imported = import_motion3(&doc, MOTION, "Swap", motion).unwrap();
    assert!(imported.diagnostics.is_empty());
    let imported = import_pose3(
        &imported.candidate,
        "00000000-0000-4000-8000-000000000d08",
        pose,
    )
    .unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut preview = MotionPreview::new(&imported.candidate);
    assert_eq!(preview.snapshot().part_opacities[part0], 1.0);
    assert_eq!(preview.snapshot().part_opacities[part1], 0.0);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    for (time, part0_expected, part1_expected, model_expected, param_expected) in [
        (0.25, 0.7, 0.5, 0.3, 0.0),
        (0.5, 0.0, 1.0, 0.4, 0.25),
        (0.75, 0.0, 1.0, 0.5, 0.5),
    ] {
        let state = preview.advance(0.25).unwrap();
        assert!((state.time - time).abs() < 0.00001);
        assert!((state.part_opacities[part0] - part0_expected).abs() < 0.00001);
        assert!((state.part_opacities[part1] - part1_expected).abs() < 0.00001);
        assert!((state.model_opacity - model_expected).abs() < 0.00001);
        assert!((state.parameters[PARAM] - param_expected).abs() < 0.00001);
        let frame = preview.evaluate_drawables().unwrap();
        assert!((frame.drawables[0].opacity - part0_expected).abs() < 0.00001);
        assert!((frame.drawables[1].opacity - part1_expected).abs() < 0.00001);
    }
}

#[test]
fn pose_uses_real_parameter_when_part_runtime_id_matches() {
    let mut doc = document();
    let part0 = "00000000-0000-4000-8000-000000000d06";
    let part1 = "00000000-0000-4000-8000-000000000d07";
    let control = "00000000-0000-4000-8000-000000000d05";
    assert!(doc
        .create_parameter(Parameter {
            id: control.into(),
            runtime_id: "Part0".into(),
            name: "Control".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    for (id, runtime) in [(part0, "Part0"), (part1, "Part1")] {
        assert!(doc
            .create_part(Part {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let motion = include_str!("../../../tests/fixtures/animation_cpu/minimal.motion3.json");
    let pose = include_str!("../../../tests/fixtures/animation_cpu/minimal.pose3.json");
    let imported = import_motion3(&doc, MOTION, "Swap", motion).unwrap();
    let imported = import_pose3(
        &imported.candidate,
        "00000000-0000-4000-8000-000000000d08",
        pose,
    )
    .unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    assert_eq!(preview.snapshot().parameters[control], 1.0);
    assert!(!preview.snapshot().part_opacity_channels.contains_key(part0));
    preview.schedule_motion(MOTION, 0.0).unwrap();
    let state = preview.advance(0.25).unwrap();
    assert_eq!(state.parameters[control], 0.0);
    assert_eq!(state.part_opacity_channels[part1], 1.0);
    assert!((state.part_opacities[part0] - 0.7).abs() < 0.00001);
    assert!((state.part_opacities[part1] - 0.5).abs() < 0.00001);
}

#[test]
fn non_looping_motion_uses_natural_end_for_clip_and_track_fades() {
    for track_fade in [None, Some(0.5)] {
        let mut source: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/animation_cpu/loop.motion3.json"
        ))
        .unwrap();
        source["Meta"]["Loop"] = false.into();
        source["Meta"]["FadeInTime"] = 0.0.into();
        source["Meta"]["FadeOutTime"] = 1.0.into();
        if let Some(fade) = track_fade {
            source["Curves"][0]["FadeOutTime"] = fade.into();
        }
        let imported = import_motion3(&document(), MOTION, "Fade", &source.to_string()).unwrap();
        let mut preview = MotionPreview::new(&imported.candidate);
        preview.schedule_motion(MOTION, 0.0).unwrap();
        preview.advance(0.0).unwrap();
        let weight = if track_fade.is_some() { 1.0 } else { 0.5 };
        assert!((preview.advance(0.5).unwrap().parameters[PARAM] - 0.5 * weight).abs() < 0.00001);
    }
}

#[test]
fn part_opacity_tracks_clamp_real_parameters() {
    let mut doc = document();
    let part = "00000000-0000-4000-8000-000000000d06";
    assert!(doc
        .create_part(Part {
            id: part.into(),
            runtime_id: "ParamX".into(),
            name: "Part".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/animation_cpu/loop.motion3.json"
    ))
    .unwrap();
    source["Curves"][0]["Target"] = "PartOpacity".into();
    source["Curves"][0]["Segments"] = serde_json::json!([0, 2, 0, 1, 2]);
    let imported = import_motion3(&doc, MOTION, "Clamp", &source.to_string()).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    assert_eq!(preview.advance(0.0).unwrap().parameters[PARAM], 1.0);
}

#[test]
fn loop_events_survive_multiple_cycles_and_failed_advance_is_atomic() {
    let mut source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/animation_cpu/loop.motion3.json"
    ))
    .unwrap();
    source["UserData"] = serde_json::json!([{"Time":0.5,"Value":"tick"}]);
    source["Meta"]["UserDataCount"] = 1.into();
    source["Meta"]["TotalUserDataSize"] = 4.into();
    let imported = import_motion3(&document(), MOTION, "Events", &source.to_string()).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    preview.advance(0.0).unwrap();
    assert_eq!(preview.advance(0.75).unwrap().fired_events.len(), 1);
    assert_eq!(preview.advance(3.0).unwrap().fired_events.len(), 2);
    assert!(preview.advance(0.0).unwrap().fired_events.is_empty());
    let before = preview.snapshot().clone();
    assert_eq!(
        preview.advance(f32::MAX).unwrap_err(),
        kasane_animation::AnimationError::EventLimit
    );
    assert_eq!(preview.snapshot(), &before);
}

#[test]
fn editing_one_track_preserves_shared_snapshots_and_other_track_storage() {
    use std::sync::Arc;
    let imported = import_motion3(
        &document(),
        MOTION,
        "Shared",
        include_str!("../../../tests/fixtures/animation_cpu/loop.motion3.json"),
    )
    .unwrap();
    let mut original = imported.candidate;
    let mut clip = original.get_motion(MOTION).unwrap().clone();
    let mut second = clip.tracks[0].clone();
    second.id = "00000000-0000-4000-8000-000000000d99".into();
    clip.tracks.push(second);
    assert!(original.replace_motion(clip).status.is_ok());
    let saved = original.checkpoint();
    let mut candidate = original.fork_candidate();
    assert!(std::ptr::eq(
        original.get_motion(MOTION).unwrap(),
        candidate.get_motion(MOTION).unwrap()
    ));
    let mut clip = candidate.get_motion(MOTION).unwrap().clone();
    let before = original.get_motion(MOTION).unwrap();
    let segment = &mut Arc::make_mut(&mut clip.tracks[0].segments)[0];
    *segment = kasane_core::document::MotionSegment::Linear {
        end: kasane_core::document::MotionPoint {
            time: 1.0,
            value: 0.25,
        },
    };
    assert!(!Arc::ptr_eq(
        &clip.tracks[0].segments,
        &before.tracks[0].segments
    ));
    assert!(Arc::ptr_eq(
        &clip.tracks[1].segments,
        &before.tracks[1].segments
    ));
    assert!(candidate.replace_motion(clip).status.is_ok());
    assert_eq!(
        original.get_motion(MOTION).unwrap().tracks[0].segments[0]
            .end()
            .value,
        1.0
    );
    assert_eq!(
        candidate.get_motion(MOTION).unwrap().tracks[0].segments[0]
            .end()
            .value,
        0.25
    );
    assert!(saved.estimated_bytes() > 0);
}
