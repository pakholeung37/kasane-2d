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
    preview.clear_seek_cache();
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
fn event_cursor_and_virtual_target_coverage_are_explicit() {
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
    preview
        .schedule_motion("00000000-0000-4000-8000-000000000d04", 0.0)
        .unwrap();
    assert!(!preview.advance(0.5).unwrap().coverage.is_empty());
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

fn registered_document(track_fades: bool) -> Document {
    use kasane_core::document::{MotionGroup, MotionRegistration};
    let mut curve =
        serde_json::json!({"Target":"Parameter", "Id":"ParamX", "Segments":[0,1,0,4,1]});
    if track_fades {
        curve["FadeInTime"] = 0.into();
        curve["FadeOutTime"] = 0.into();
    }
    let source = serde_json::json!({"Version":3,"Meta":{"Duration":4,"Fps":30,"Loop":true,
        "AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,
        "FadeInTime":2,"FadeOutTime":2,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[curve]});
    let mut doc = import_motion3(&document(), MOTION, "Registered", &source.to_string())
        .unwrap()
        .candidate;
    assert!(doc
        .set_motion_groups(vec![MotionGroup {
            name: "Idle".into(),
            entries: [
                (None, None),
                (Some(0.0), Some(0.0)),
                (Some(1.0), Some(0.25))
            ]
            .into_iter()
            .map(|(fade_in, fade_out)| MotionRegistration {
                clip_id: MOTION.into(),
                fade_in,
                fade_out,
                sound: None,
                extensions: Default::default(),
            })
            .collect()
        }])
        .status
        .is_ok());
    doc
}

#[test]
fn registration_fades_are_per_activation_and_track_fades_take_precedence() {
    for (entry, expected) in [(0, 0.14644662), (1, 1.0), (2, 0.5)] {
        let doc = registered_document(false);
        let mut preview = MotionPreview::new(&doc);
        preview.schedule_motion_entry("Idle", entry, 0.0).unwrap();
        preview.advance(0.0).unwrap();
        assert!((preview.advance(0.5).unwrap().parameters[PARAM] - expected).abs() < 1e-6);
        assert_eq!(doc.get_motion(MOTION).unwrap().fade_in, Some(2.0));
    }
    let mut preview = MotionPreview::new(&registered_document(true));
    preview.schedule_motion_entry("Idle", 2, 0.0).unwrap();
    assert_eq!(preview.advance(0.0).unwrap().parameters[PARAM], 1.0);

    // The same clip's outgoing registration must keep its own short fade-out.
    let mut preview = MotionPreview::new(&registered_document(false));
    preview.schedule_motion_entry("Idle", 2, 0.0).unwrap();
    preview.schedule_motion_entry("Idle", 0, 1.0).unwrap();
    preview.advance(0.0).unwrap();
    preview.advance(1.0).unwrap();
    assert_eq!(preview.snapshot().active_motions.len(), 2);
    assert_eq!(preview.advance(0.3).unwrap().active_motions.len(), 1);
    let mut preview = MotionPreview::new(&registered_document(false));
    preview.schedule_motion_entry("Idle", 0, 0.0).unwrap();
    preview.schedule_motion_entry("Idle", 1, 1.0).unwrap();
    preview.advance(0.0).unwrap();
    preview.advance(1.0).unwrap();
    assert_eq!(preview.advance(0.3).unwrap().active_motions.len(), 2);
}

fn combined_cache_preview() -> MotionPreview {
    let mut doc = document();
    assert!(doc
        .create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000e01".into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            minimum: -1.0,
            maximum: 1.0,
            ..Default::default()
        })
        .status
        .is_ok());
    for (id, runtime) in [
        ("00000000-0000-4000-8000-000000000e02", "Part0"),
        ("00000000-0000-4000-8000-000000000e03", "Part1"),
    ] {
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
    let source = serde_json::json!({"Version":3,"Meta":{"Duration":2,"Fps":30,"Loop":true,
        "AreBeziersRestricted":true,"CurveCount":2,"TotalSegmentCount":3,"TotalPointCount":5,
        "FadeInTime":0,"FadeOutTime":0,"UserDataCount":1,"TotalUserDataSize":4},
        "Curves":[{"Target":"Parameter","Id":"ParamX","Segments":[0,0,0,2,1]},
        {"Target":"PartOpacity","Id":"Part1","Segments":[0,0,2,0.8,1,2,2,0]}],
        "UserData":[{"Time":59.0/60.0,"Value":"tick"}]});
    doc = import_motion3(&doc, MOTION, "Cache", &source.to_string())
        .unwrap()
        .candidate;
    doc = import_pose3(
        &doc,
        "00000000-0000-4000-8000-000000000e04",
        include_str!("../../../tests/fixtures/animation_cpu/minimal.pose3.json"),
    )
    .unwrap()
    .candidate;
    doc = import_physics3(
        &doc,
        "00000000-0000-4000-8000-000000000e05",
        include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json"),
    )
    .unwrap()
    .candidate;
    let expression = "00000000-0000-4000-8000-000000000e06";
    doc = import_expression3(&doc, expression, "Add", r#"{"Type":"Live2D Expression","FadeInTime":0.7,"FadeOutTime":0.8,"Parameters":[{"Id":"ParamX","Value":0.2,"Blend":"Add"}]}"#).unwrap().candidate;
    let mut preview = MotionPreview::new(&doc);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    preview.schedule_motion(MOTION, 2.4).unwrap();
    preview.schedule_expression(expression, 0.2).unwrap();
    preview.schedule_expression(expression, 1.5).unwrap();
    preview.schedule_parameter_input(PARAM, 1.1, 0.8).unwrap();
    preview
}

#[test]
fn cached_seek_matches_cold_replay_for_complete_runtime_state() {
    let mut cached = combined_cache_preview();
    let mut cold = combined_cache_preview();
    cold.set_seek_cache_budget(0);
    // Warm checkpoints, then cover exact hits, fractional tails, backward and zero seeks.
    for time in [3.37, 1.0, 1.01, 2.0, 2.7, 0.0, 0.001, 4.1, 1.0, 60.0, 61.0] {
        assert_eq!(
            cached.seek(time).unwrap(),
            cold.seek(time).unwrap(),
            "time={time}"
        );
        assert!(
            cached.seek_cache_stats().estimated_bytes <= cached.seek_cache_stats().budget_bytes
        );
    }
    assert_eq!(cached.seek_cache_stats().last_restored_time, 60.0);
    assert_eq!(cached.seek_cache_stats().last_replayed_steps, 60);
    assert_eq!(cold.seek_cache_stats().last_replayed_steps, 3660);
    assert_eq!(cold.seek_cache_stats().checkpoints, 0);
    // Noncanonical mutations must never seed checkpoints.
    cached.advance(0.031).unwrap();
    cached.stabilize_physics();
    assert_eq!(cached.seek(62.0).unwrap(), cold.seek(62.0).unwrap());
    cached.seek(1.0).unwrap();
    cold.seek(1.0).unwrap();
    for preview in [&mut cached, &mut cold] {
        preview.schedule_parameter_input(PARAM, 1.0, 0.3).unwrap();
        assert_eq!(preview.seek_cache_stats().checkpoints, 0);
    }
    assert_eq!(cached.seek(2.0).unwrap(), cold.seek(2.0).unwrap());
    cached.set_base_parameter(PARAM, 0.4).unwrap();
    cold.set_base_parameter(PARAM, 0.4).unwrap();
    assert_eq!(cached.seek_cache_stats().checkpoints, 0);
    assert_eq!(cached.seek(2.0).unwrap(), cold.seek(2.0).unwrap());
}

#[test]
fn cache_eviction_cancellation_and_failed_schedule_are_atomic() {
    let mut preview = combined_cache_preview();
    preview.seek(1.0).unwrap();
    let one = preview.seek_cache_stats().estimated_bytes;
    preview.set_seek_cache_budget(one * 2);
    preview.seek(20.0).unwrap();
    assert!(preview.seek_cache_stats().checkpoints < 20);
    assert!(preview.seek_cache_stats().estimated_bytes <= one * 2);
    let state = preview.snapshot().clone();
    let stats = preview.seek_cache_stats();
    assert!(preview.schedule_motion_entry("missing", 0, 21.0).is_err());
    assert!(preview.schedule_motion("missing", 21.0).is_err());
    assert!(preview.schedule_expression("missing", 21.0).is_err());
    assert!(preview.schedule_parameter_input(PARAM, 0.0, 1.0).is_err());
    assert_eq!(preview.seek_cache_stats(), stats);
    for target in [20.0, 22.0, 0.9] {
        assert_eq!(
            preview
                .seek_with_progress(target, |done, total| total != 0
                    && done < if target == 22.0 { 65 } else { 3 })
                .unwrap_err(),
            kasane_animation::AnimationError::SeekCancelled
        );
        assert_eq!(preview.snapshot(), &state);
        assert_eq!(preview.seek_cache_stats(), stats);
    }
    let mut calls = Vec::new();
    preview
        .seek_with_progress(20.0, |done, total| {
            calls.push((done, total));
            true
        })
        .unwrap();
    assert_eq!(calls, [(0, 0)]);
    preview.set_seek_cache_budget(1);
    preview.seek(3.0).unwrap();
    assert_eq!(preview.seek_cache_stats().estimated_bytes, 0);
    assert_eq!(preview.seek_cache_stats().checkpoints, 0);
    preview.reset();
    assert_eq!(preview.seek_cache_stats().budget_bytes, 1);
}
