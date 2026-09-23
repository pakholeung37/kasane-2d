use super::*;

pub(crate) fn scene_form_from_tuple(kind: &str, value: SceneFormTuple) -> PyResult<SceneKeyform> {
    let (keys, positions, pose, draw_order, appearance) = value;
    match kind {
        "warp" if pose.is_none() && draw_order.is_none() && appearance.is_some() => {
            Ok(SceneKeyform::Warp(WarpKeyform {
                keys,
                positions: positions
                    .into_iter()
                    .map(|(x, y)| Vec2::new(x, y))
                    .collect(),
                appearance: appearance_from_tuple(appearance.expect("checked")),
            }))
        }
        "rotation"
            if positions.is_empty()
                && draw_order.is_none()
                && pose.is_some()
                && appearance.is_some() =>
        {
            Ok(SceneKeyform::Rotation(RotationKeyform {
                keys,
                rotation: pose_from_tuple(pose.expect("checked")),
                appearance: appearance_from_tuple(appearance.expect("checked")),
            }))
        }
        "part"
            if positions.is_empty()
                && pose.is_none()
                && appearance.is_none()
                && draw_order.is_some() =>
        {
            Ok(SceneKeyform::Part(PartKeyform {
                keys,
                draw_order: draw_order.expect("checked"),
            }))
        }
        _ => Err(PyValueError::new_err(
            "Scene keyform does not match its track kind",
        )),
    }
}

pub(crate) fn scene_binding_from_tuples(
    id: String,
    kind: &str,
    target_id: String,
    axes: Vec<(String, Vec<f32>)>,
    forms: Vec<SceneFormTuple>,
) -> PyResult<SceneBinding> {
    let forms: Vec<_> = forms
        .into_iter()
        .map(|form| scene_form_from_tuple(kind, form))
        .collect::<PyResult<_>>()?;
    let track = match kind {
        "warp" => SceneTrack::Warp {
            target_id: target_id.into(),
            keyforms: forms
                .into_iter()
                .map(|form| match form {
                    SceneKeyform::Warp(form) => form,
                    _ => unreachable!(),
                })
                .collect(),
        },
        "rotation" => SceneTrack::Rotation {
            target_id: target_id.into(),
            keyforms: forms
                .into_iter()
                .map(|form| match form {
                    SceneKeyform::Rotation(form) => form,
                    _ => unreachable!(),
                })
                .collect(),
        },
        "part" => SceneTrack::Part {
            target_id: target_id.into(),
            keyforms: forms
                .into_iter()
                .map(|form| match form {
                    SceneKeyform::Part(form) => form,
                    _ => unreachable!(),
                })
                .collect(),
        },
        _ => return Err(PyValueError::new_err("Unknown scene track kind")),
    };
    Ok(SceneBinding {
        id,
        axes: axes
            .into_iter()
            .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
            .collect(),
        track,
    })
}

pub(crate) fn scene_binding_tuple(binding: SceneBinding, version: Version) -> SceneBindingTuple {
    let (kind, target_id, forms): (&str, String, Vec<SceneFormTuple>) = match binding.track {
        SceneTrack::Warp {
            target_id,
            keyforms,
        } => (
            "warp",
            target_id.as_str().into(),
            keyforms
                .into_iter()
                .map(|form| {
                    (
                        form.keys,
                        form.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                        None,
                        None,
                        Some(appearance_tuple(form.appearance)),
                    )
                })
                .collect(),
        ),
        SceneTrack::Rotation {
            target_id,
            keyforms,
        } => (
            "rotation",
            target_id.as_str().into(),
            keyforms
                .into_iter()
                .map(|form| {
                    (
                        form.keys,
                        Vec::new(),
                        Some(pose_tuple(form.rotation)),
                        None,
                        Some(appearance_tuple(form.appearance)),
                    )
                })
                .collect(),
        ),
        SceneTrack::Part {
            target_id,
            keyforms,
        } => (
            "part",
            target_id.as_str().into(),
            keyforms
                .into_iter()
                .map(|form| (form.keys, Vec::new(), None, Some(form.draw_order), None))
                .collect(),
        ),
    };
    (
        binding.id,
        binding
            .axes
            .into_iter()
            .map(|axis| (axis.parameter_id, axis.keys))
            .collect(),
        kind.into(),
        target_id,
        forms,
        version_tuple(version),
    )
}

pub(crate) fn transform_tuple(transform: Transform, version: Version) -> TransformTuple {
    let (kind, rotation, warp) = match transform.data {
        TransformData::Rotation(data) => (
            "rotation".to_owned(),
            Some((
                data.base_angle,
                (
                    data.pose.origin.x,
                    data.pose.origin.y,
                    data.pose.angle,
                    data.pose.scale,
                    data.pose.reflect_x,
                    data.pose.reflect_y,
                ),
            )),
            None,
        ),
        TransformData::Warp(data) => (
            "warp".to_owned(),
            None,
            Some((
                data.rows,
                data.columns,
                data.quad,
                data.points.into_iter().map(|p| (p.x, p.y)).collect(),
            )),
        ),
    };
    (
        transform.id,
        transform.runtime_id,
        transform.name,
        transform.part_id.map(|id| id.as_str().to_owned()),
        transform.parent_id.map(|id| id.as_str().to_owned()),
        kind,
        rotation,
        warp,
        transform.enabled,
        appearance_tuple(transform.appearance),
        version_tuple(version),
    )
}
