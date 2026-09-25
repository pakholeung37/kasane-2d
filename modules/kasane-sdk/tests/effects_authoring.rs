use kasane_core::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    Canvas, DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, Offscreen, OffscreenKeyform,
    Parameter, ParameterKind, Part, PartKeyform, PreviewValues, SceneBinding, SceneTrack, Vec2,
};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, ObjectKind};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

#[test]
fn part_binding_and_offscreen_mapping_resize_together() {
    let mut sdk = AuthoringSession::new(
        &id(101),
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    sdk.edit("base", None, |edit| {
        edit.create_parameter(Parameter {
            id: id(102),
            minimum: 0.0,
            maximum: 1.0,
            ..Default::default()
        })?;
        edit.create_part(Part {
            id: id(103),
            ..Default::default()
        })?;
        edit.create_scene_binding(SceneBinding {
            id: id(104),
            axes: vec![BindingAxis {
                parameter_id: id(102),
                keys: vec![0.0, 1.0],
            }],
            track: SceneTrack::Part {
                target_id: id(103).into(),
                keyforms: vec![
                    PartKeyform {
                        keys: vec![0.0],
                        draw_order: 0.0,
                    },
                    PartKeyform {
                        keys: vec![1.0],
                        draw_order: 10.0,
                    },
                ],
            },
        })?;
        edit.create_offscreen(Offscreen {
            id: id(105),
            runtime_id: id(105),
            name: String::new(),
            part_id: id(103),
            blend_mode: 0,
            flags: 0,
            masks: vec![],
            part_keyform_indices: vec![0, 1],
            keyforms: vec![
                OffscreenKeyform {
                    opacity: 0.5,
                    ..Default::default()
                },
                OffscreenKeyform {
                    opacity: 1.0,
                    ..Default::default()
                },
            ],
        })
    })
    .unwrap();
    let version = sdk.version();
    let mut binding = sdk.scene_binding(&id(104)).unwrap();
    binding.axes[0].keys = vec![0.0, 0.5, 1.0];
    binding.track = SceneTrack::Part {
        target_id: id(103).into(),
        keyforms: [0.0, 0.5, 1.0]
            .into_iter()
            .map(|key| PartKeyform {
                keys: vec![key],
                draw_order: key * 10.0,
            })
            .collect(),
    };
    let mut offscreen = sdk.offscreen(&id(105)).unwrap();
    offscreen.part_keyform_indices = vec![0, -1, 1];
    let mut invalid = sdk.begin_edit("invalid independent resize", None).unwrap();
    assert_eq!(
        invalid
            .replace_scene_binding(binding.clone())
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_LENGTH"
    );
    assert_eq!(invalid.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);
    sdk.edit("joint resize", Some(version), |edit| {
        edit.replace_part_binding_with_offscreen(binding, offscreen)
    })
    .unwrap();
    assert_eq!(sdk.scene_binding(&id(104)).unwrap().track.len(), 3);
    assert_eq!(
        sdk.offscreen(&id(105)).unwrap().part_keyform_indices,
        [0, -1, 1]
    );
    sdk.undo().unwrap();
    assert_eq!(sdk.scene_binding(&id(104)).unwrap().track.len(), 2);
}

#[test]
fn effect_families_share_one_publish_and_failed_replacement_preserves_it() {
    let mut sdk = AuthoringSession::new(
        &id(1),
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(&id(2), "texture", &path).unwrap();
    let mesh_a = rectangle_mesh(
        &id(3),
        "a",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let mesh_b = rectangle_mesh(
        &id(4),
        "b",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let before = sdk.version();
    sdk.edit("effects", Some(before), |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh_a)?;
        edit.create_mesh(mesh_b)?;
        edit.create_part(Part {
            id: id(5),
            ..Default::default()
        })?;
        edit.create_parameter(Parameter {
            id: id(6),
            minimum: 0.0,
            maximum: 1.0,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        edit.create_parameter(Parameter {
            id: id(7),
            minimum: 0.0,
            maximum: 1.0,
            ..Default::default()
        })?;
        edit.create_blend_key_table(BlendShapeKeyTable {
            id: id(8),
            parameter_id: id(6),
            keys: vec![0.0, 1.0],
            base_key_idx: 0,
        })?;
        edit.create_blend_constraint(BlendShapeConstraint {
            id: id(9),
            parameter_id: id(7),
            keys: vec![0.0, 1.0],
            weights: vec![1.0, 1.0],
        })?;
        edit.create_glue(Glue {
            id: id(10),
            runtime_id: id(10),
            name: "seam".into(),
            mesh_a_id: id(3),
            mesh_b_id: id(4),
            pairs: vec![GlueVertexPair {
                vertex_a: 0,
                vertex_b: 0,
                weight_a: 1.0,
                weight_b: 1.0,
            }],
            intensity: 0.0,
            binding: None,
        })?;
        edit.create_offscreen(Offscreen {
            id: id(11),
            runtime_id: id(11),
            name: "group".into(),
            part_id: id(5),
            blend_mode: 0,
            flags: 0,
            masks: vec![],
            part_keyform_indices: vec![],
            keyforms: vec![OffscreenKeyform {
                opacity: 1.0,
                ..Default::default()
            }],
        })?;
        edit.create_blend_binding(BlendShapeBinding {
            id: id(12),
            target_id: id(3),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: id(8),
            constraint_ids: vec![id(9)],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::default(); 4],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(1.0, 0.0); 4],
                    ..Default::default()
                },
            ]),
        })
    })
    .unwrap();
    assert_eq!(sdk.version().revision, before.revision + 1);
    assert_eq!(sdk.blend_key_table_ids(), &[id(8)]);
    assert_eq!(sdk.blend_constraint_ids(), &[id(9)]);
    assert_eq!(sdk.blend_binding_ids(), &[id(12)]);
    assert_eq!(sdk.glue_ids(), &[id(10)]);
    assert_eq!(sdk.offscreen_ids(), &[id(11)]);
    assert_eq!(sdk.glue(&id(10)).unwrap().pairs.len(), 1);
    assert_eq!(sdk.offscreen(&id(11)).unwrap().part_id, id(5));
    assert_eq!(sdk.blend_key_table(&id(8)).unwrap().keys, [0.0, 1.0]);
    assert_eq!(sdk.blend_constraint(&id(9)).unwrap().weights, [1.0, 1.0]);
    assert_eq!(sdk.blend_binding(&id(12)).unwrap().target_id, id(3));
    sdk.handle(ObjectKind::BlendBinding, &id(12)).unwrap();
    assert!(sdk.evaluate(&PreviewValues::from([(id(6), 1.0)])).is_ok());

    let stable = sdk.version();
    let mut edit = sdk.begin_edit("invalid effect", None).unwrap();
    assert_eq!(
        edit.replace_blend_constraint(BlendShapeConstraint {
            id: id(9),
            parameter_id: id(7),
            keys: vec![0.0, 1.0],
            weights: vec![1.0],
        })
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_LENGTH"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), stable);

    sdk.undo().unwrap();
    assert!(sdk.blend_binding_ids().is_empty());
    assert!(sdk.offscreen_ids().is_empty());
    sdk.redo().unwrap();
    assert_eq!(sdk.blend_binding_ids(), &[id(12)]);
}
