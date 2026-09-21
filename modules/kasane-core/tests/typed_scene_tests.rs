use kasane_core::*;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn document() -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(id(1), Canvas::new(100., 100., Vec2::default(), 1.))
        .is_ok());
    assert!(doc
        .create_parameter(Parameter {
            id: id(2),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_transform(Transform {
            id: id(3),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(0., 1.),
                    Vec2::new(1., 1.)
                ],
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    doc
}
fn axes() -> Vec<BindingAxis> {
    vec![BindingAxis {
        parameter_id: id(2),
        keys: vec![-1., 1.],
    }]
}

#[test]
fn wrong_track_and_single_form_types_fail_without_mutation() {
    let mut doc = document();
    let before = doc.clone();
    let invalid = SceneBinding {
        id: id(4),
        axes: axes(),
        track: SceneTrack::Rotation {
            target_id: id(3).into(),
            keyforms: vec![
                RotationKeyform {
                    keys: vec![-1.],
                    ..Default::default()
                },
                RotationKeyform {
                    keys: vec![1.],
                    ..Default::default()
                },
            ],
        },
    };
    assert_eq!(
        doc.create_scene_binding(invalid).status.code,
        "INVALID_BINDING_TARGET"
    );
    assert!(doc.same_content(&before));
    let positions = doc
        .get_transform(&id(3))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    assert!(doc
        .create_scene_binding(SceneBinding {
            id: id(4),
            axes: axes(),
            track: SceneTrack::Warp {
                target_id: id(3).into(),
                keyforms: [1., -1.]
                    .into_iter()
                    .map(|key| WarpKeyform {
                        keys: vec![key],
                        positions: positions.clone(),
                        ..Default::default()
                    })
                    .collect(),
            },
        })
        .status
        .is_ok());
    let binding = doc.get_scene_binding(&id(4)).unwrap();
    assert_eq!(binding.track.sample(0).keys, [-1.]);
    assert_eq!(binding.track.sample(1).keys, [1.]);
    let before = doc.clone();
    assert_eq!(
        doc.set_scene_keyform(
            &id(4),
            SceneKeyform::Part(PartKeyform {
                keys: vec![-1.],
                draw_order: 2.,
            })
        )
        .status
        .code,
        "INVALID_KEYFORM_TYPE"
    );
    assert!(doc.same_content(&before));
    assert_eq!(doc.revision(), before.revision());
}

#[test]
fn typed_parent_references_and_rotation_precision_survive_edits() {
    let mut doc = document();
    let origin = PreciseVec2::new(16777217.125, -16777217.375);
    let rotation = Transform {
        id: id(5),
        parent_id: Some(id(3).into()),
        data: TransformData::Rotation(RotationTransform {
            base_angle: 12.,
            pose: RotationPose {
                origin,
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    assert!(doc.create_transform(rotation).status.is_ok());
    let mut changed = doc.get_transform(&id(5)).unwrap().clone();
    assert_eq!(changed.rotation().unwrap().pose.origin, origin);
    changed.parent_id = Some("".into());
    assert_eq!(doc.replace_transform(changed).status.code, "INVALID_ID");
    let mut changed = doc.get_transform(&id(5)).unwrap().clone();
    changed.parent_id = None;
    assert!(doc.replace_transform(changed).status.is_ok());
    assert_eq!(
        doc.get_transform(&id(5))
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .origin,
        origin
    );
    assert!(doc.get_transform(&id(5)).unwrap().parent_id.is_none());
}
