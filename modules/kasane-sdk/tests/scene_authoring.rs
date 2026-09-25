use std::sync::Arc;

use kasane_core::{
    BindingAxis, Canvas, Parameter, Part, PartKeyform, PreviewValues, RotationKeyform,
    RotationPose, RotationTransform, SceneBinding, SceneKeyform, SceneTrack, Transform,
    TransformData, TransformId, Vec2, WarpKeyform, WarpTransform,
};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, ObjectKind, SourceSpace};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        &id(1),
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(&id(2), "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        &id(3),
        "mesh",
        &id(2),
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
    )
    .unwrap();
    sdk.edit("fixture", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })
    .unwrap();
    sdk
}

fn warp() -> Transform {
    Transform {
        id: id(6),
        part_id: Some(id(4).into()),
        parent_id: Some(id(5).into()),
        data: TransformData::Warp(WarpTransform {
            rows: 1,
            columns: 1,
            quad: true,
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ],
        }),
        ..Default::default()
    }
}

#[test]
fn parts_transforms_and_distinct_parent_relations_publish_atomically() {
    let mut sdk = session();
    let original = sdk.version();
    let (_, receipt) = sdk
        .edit("hierarchy", Some(original), |edit| {
            edit.create_part(Part {
                id: id(4),
                name: "root".into(),
                ..Default::default()
            })?;
            edit.create_part(Part {
                id: id(11),
                name: "child".into(),
                ..Default::default()
            })?;
            edit.set_organization_parent(&id(11), &id(4))?;
            edit.create_transform(Transform {
                id: id(5),
                part_id: Some(id(4).into()),
                data: TransformData::Rotation(RotationTransform::default()),
                ..Default::default()
            })?;
            edit.create_transform(warp())?;
            edit.set_transform_part(&id(6), Some(id(11).into()))?;
            edit.set_mesh_part(&id(3), &id(11))?;
            edit.update_rotation(
                &id(5),
                RotationTransform {
                    base_angle: 5.0,
                    ..Default::default()
                },
            )?;
            let shifted = warp()
                .warp()
                .unwrap()
                .points
                .iter()
                .map(|p| Vec2::new(p.x + 0.1, p.y))
                .collect();
            edit.update_warp_points(&id(6), shifted)?;
            edit.set_deform_parent(&id(3), &id(6))
        })
        .unwrap();
    assert_eq!(receipt.after.revision, original.revision + 1);
    assert_eq!(sdk.part_ids(), &[id(4), id(11)]);
    assert_eq!(sdk.part(&id(11)).unwrap().parent_id, id(4));
    assert_eq!(sdk.transform_ids(), &[id(5), id(6)]);
    assert_eq!(
        sdk.geometry(&id(3)).unwrap().space,
        SourceSpace::ParentLocal(id(6))
    );
    assert_eq!(sdk.transform(&id(6)).unwrap().parent(), id(5));
    assert_eq!(sdk.transform(&id(6)).unwrap().part(), id(11));
    assert_eq!(sdk.mesh(&id(3)).unwrap().part_id, id(11));
    assert_eq!(
        sdk.transform(&id(5))
            .unwrap()
            .rotation()
            .unwrap()
            .base_angle,
        5.0
    );
    assert_eq!(
        sdk.transform(&id(6)).unwrap().warp().unwrap().points[0].x,
        0.1
    );
    assert!(sdk.evaluate(&PreviewValues::new()).is_ok());
    let handle = sdk.handle(ObjectKind::Transform, &id(6)).unwrap();

    let mut edit = sdk.begin_edit("cycle", None).unwrap();
    assert_eq!(
        edit.set_transform_parent(&id(5), Some(TransformId::from(id(6))))
            .unwrap_err()
            .code
            .as_ref(),
        "RELATION_CYCLE"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), receipt.after);
    sdk.resolve_handle(&handle).unwrap();

    let mut edit = sdk.begin_edit("wrong transform type", None).unwrap();
    assert_eq!(
        edit.update_warp_points(&id(5), vec![])
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_TRANSFORM_TYPE"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");

    let mut edit = sdk.begin_edit("part cycle", None).unwrap();
    assert_eq!(
        edit.set_organization_parent(&id(4), &id(11))
            .unwrap_err()
            .code
            .as_ref(),
        "RELATION_CYCLE"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");

    sdk.undo().unwrap();
    assert!(sdk.part_ids().is_empty());
    assert!(sdk.transform_ids().is_empty());
    assert_eq!(
        sdk.geometry(&id(3)).unwrap().space,
        SourceSpace::CanvasPixels
    );
    sdk.redo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}

#[test]
fn scene_tracks_require_complete_typed_forms_and_support_replacement() {
    let mut sdk = session();
    let axis = BindingAxis {
        parameter_id: id(7),
        keys: vec![0.0, 1.0],
    };
    sdk.edit("scene", None, |edit| {
        edit.create_parameter(Parameter {
            id: id(7),
            minimum: 0.0,
            maximum: 1.0,
            ..Default::default()
        })?;
        edit.create_part(Part {
            id: id(4),
            name: "part".into(),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: id(5),
            data: TransformData::Rotation(RotationTransform::default()),
            ..Default::default()
        })?;
        edit.create_transform(warp())?;
        edit.create_scene_binding(SceneBinding {
            id: id(8),
            axes: vec![axis.clone()],
            track: SceneTrack::Part {
                target_id: id(4).into(),
                keyforms: vec![
                    PartKeyform {
                        keys: vec![1.0],
                        draw_order: 10.0,
                    },
                    PartKeyform {
                        keys: vec![0.0],
                        draw_order: 0.0,
                    },
                ],
            },
        })?;
        edit.create_scene_binding(SceneBinding {
            id: id(9),
            axes: vec![axis.clone()],
            track: SceneTrack::Rotation {
                target_id: id(5).into(),
                keyforms: vec![0.0, 1.0]
                    .into_iter()
                    .map(|key| RotationKeyform {
                        keys: vec![key],
                        rotation: RotationPose {
                            angle: key * 30.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .collect(),
            },
        })?;
        let base = warp().warp().unwrap().points.clone();
        edit.create_scene_binding(SceneBinding {
            id: id(10),
            axes: vec![axis],
            track: SceneTrack::Warp {
                target_id: id(6).into(),
                keyforms: vec![0.0, 1.0]
                    .into_iter()
                    .map(|key| WarpKeyform {
                        keys: vec![key],
                        positions: base.iter().map(|p| Vec2::new(p.x + key, p.y)).collect(),
                        ..Default::default()
                    })
                    .collect(),
            },
        })
    })
    .unwrap();
    assert_eq!(sdk.scene_binding_ids(), &[id(8), id(9), id(10)]);
    assert_eq!(sdk.binding_for_scene(&id(6)).unwrap().id, id(10));
    assert_eq!(
        sdk.scene_binding(&id(8)).unwrap().track.sample(0).keys,
        [0.0]
    );
    assert!(sdk.evaluate(&PreviewValues::from([(id(7), 0.5)])).is_ok());

    let mut bad = sdk.begin_edit("wrong type", None).unwrap();
    assert_eq!(
        bad.set_scene_keyform(
            &id(9),
            SceneKeyform::Part(PartKeyform {
                keys: vec![0.0],
                draw_order: 3.0,
            })
        )
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_KEYFORM_TYPE"
    );
    assert_eq!(bad.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");

    sdk.edit("replace form", None, |edit| {
        edit.set_scene_keyform(
            &id(8),
            SceneKeyform::Part(PartKeyform {
                keys: vec![1.0],
                draw_order: 20.0,
            }),
        )
    })
    .unwrap();
    assert_eq!(
        sdk.scene_binding(&id(8))
            .unwrap()
            .track
            .sample(1)
            .draw_order,
        20.0
    );
    sdk.undo().unwrap();
    assert_eq!(
        sdk.scene_binding(&id(8))
            .unwrap()
            .track
            .sample(1)
            .draw_order,
        10.0
    );
}

#[test]
fn incomplete_scene_binding_rolls_back_its_batch() {
    let mut sdk = session();
    let before = sdk.version();
    let mut edit = sdk.begin_edit("incomplete", None).unwrap();
    edit.create_parameter(Parameter {
        id: id(7),
        minimum: 0.0,
        maximum: 1.0,
        ..Default::default()
    })
    .unwrap();
    edit.create_part(Part {
        id: id(4),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        edit.create_scene_binding(SceneBinding {
            id: id(8),
            axes: vec![BindingAxis {
                parameter_id: id(7),
                keys: vec![0.0, 1.0]
            }],
            track: SceneTrack::Part {
                target_id: id(4).into(),
                keyforms: vec![PartKeyform {
                    keys: vec![0.0],
                    draw_order: 0.0
                }],
            },
        })
        .unwrap_err()
        .code
        .as_ref(),
        "INCOMPLETE_KEYFORMS"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), before);
    assert!(sdk.parameter_ids().is_empty());
    assert!(sdk.part_ids().is_empty());
}

#[test]
fn session_preview_caches_frames_and_rejects_bad_requests_without_state_change() {
    let mut sdk = session();
    sdk.edit("parameter", None, |edit| {
        edit.create_parameter(Parameter {
            id: id(7),
            minimum: 0.0,
            maximum: 1.0,
            ..Default::default()
        })
    })
    .unwrap();
    assert!(sdk
        .set_preview_values(PreviewValues::from([(id(7), 0.5)]))
        .unwrap());
    let first = sdk.preview_frame().unwrap();
    assert!(Arc::ptr_eq(&first, &sdk.preview_frame().unwrap()));
    let count = sdk.preview_evaluation_count();
    sdk.evaluate(&PreviewValues::from([(id(7), 1.0)])).unwrap();
    assert_eq!(sdk.preview_evaluation_count(), count);
    assert_eq!(sdk.preview_values()[&id(7)], 0.5);
    let revision = sdk.preview_revision();
    assert_eq!(
        sdk.set_preview_values(PreviewValues::from([(id(7), f32::NAN)]))
            .unwrap_err()
            .code
            .as_ref(),
        "NON_FINITE"
    );
    assert_eq!(sdk.preview_revision(), revision);
    assert!(Arc::ptr_eq(&first, &sdk.preview_frame().unwrap()));
    assert!(sdk.set_preview_parameter(&id(7), 0.75).unwrap());
    assert_eq!(sdk.preview_values()[&id(7)], 0.75);
    assert!(!sdk.set_preview_parameter(&id(7), 0.75).unwrap());
    sdk.set_preview_parameter(&id(7), 0.5).unwrap();
    let restored = sdk.preview_frame().unwrap();

    sdk.edit("rename", None, |edit| edit.rename_mesh(&id(3), "renamed"))
        .unwrap();
    assert!(Arc::ptr_eq(&restored, &sdk.preview_frame().unwrap()));
    let geometry = sdk.geometry(&id(3)).unwrap();
    let shifted: Vec<_> = geometry
        .positions
        .iter()
        .map(|p| Vec2::new(p.x + 1.0, p.y))
        .collect();
    sdk.edit("move", None, |edit| {
        edit.update_positions(&id(3), &geometry.vertex_ids, &shifted)
    })
    .unwrap();
    assert!(!Arc::ptr_eq(&restored, &sdk.preview_frame().unwrap()));
    sdk.undo().unwrap();
    assert_eq!(sdk.preview_values()[&id(7)], 0.5);
    assert!(sdk.reset_preview_values().unwrap());
    assert!(sdk.preview_values().is_empty());
}
