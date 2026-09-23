use super::*;

pub(crate) fn blend_binding_from_tuple(data: BlendBindingDataTuple) -> PyResult<BlendShapeBinding> {
    let (id, target_id, target_kind, key_table_id, constraint_ids, forms) = data;
    let (target_kind, keyforms) = match target_kind.as_str() {
        "mesh" => (
            BlendShapeTargetKind::Mesh,
            DeltaKeyforms::Mesh(
                forms
                    .into_iter()
                    .map(
                        |(positions, _, _, _, opacity, draw_order, _, multiply, screen)| {
                            DeltaMeshKeyform {
                                positions: positions
                                    .into_iter()
                                    .map(|(x, y)| Vec2::new(x, y))
                                    .collect(),
                                opacity,
                                draw_order,
                                multiply: multiply.map(|(r, g, b)| [r, g, b]),
                                screen: screen.map(|(r, g, b)| [r, g, b]),
                            }
                        },
                    )
                    .collect(),
            ),
        ),
        "warp" => (
            BlendShapeTargetKind::Warp,
            DeltaKeyforms::Warp(
                forms
                    .into_iter()
                    .map(
                        |(points, _, _, _, opacity, _, _, multiply, screen)| DeltaWarpKeyform {
                            points: points.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
                            opacity,
                            multiply: multiply.map(|(r, g, b)| [r, g, b]),
                            screen: screen.map(|(r, g, b)| [r, g, b]),
                        },
                    )
                    .collect(),
            ),
        ),
        "rotation" => (
            BlendShapeTargetKind::Rotation,
            DeltaKeyforms::Rotation(
                forms
                    .into_iter()
                    .map(
                        |(_, origin, angle, scale, opacity, _, _, multiply, screen)| {
                            DeltaRotationKeyform {
                                origin: origin.map(|(x, y)| Vec2::new(x, y)),
                                angle,
                                scale,
                                opacity,
                                multiply: multiply.map(|(r, g, b)| [r, g, b]),
                                screen: screen.map(|(r, g, b)| [r, g, b]),
                            }
                        },
                    )
                    .collect(),
            ),
        ),
        "part" => (
            BlendShapeTargetKind::Part,
            DeltaKeyforms::Part(
                forms
                    .into_iter()
                    .map(|(_, _, _, _, _, draw_order, _, _, _)| {
                        draw_order
                            .map(|draw_order| DeltaPartKeyform { draw_order })
                            .ok_or_else(|| PyValueError::new_err("Part delta requires draw_order"))
                    })
                    .collect::<PyResult<_>>()?,
            ),
        ),
        "glue" => (
            BlendShapeTargetKind::Glue,
            DeltaKeyforms::Glue(
                forms
                    .into_iter()
                    .map(|(_, _, _, _, _, _, intensity, _, _)| {
                        intensity
                            .map(|intensity| DeltaGlueKeyform { intensity })
                            .ok_or_else(|| PyValueError::new_err("Glue delta requires intensity"))
                    })
                    .collect::<PyResult<_>>()?,
            ),
        ),
        "offscreen" => (
            BlendShapeTargetKind::Offscreen,
            DeltaKeyforms::Offscreen(
                forms
                    .into_iter()
                    .map(|(_, _, _, _, opacity, _, _, multiply, screen)| {
                        opacity
                            .map(|opacity| DeltaOffscreenKeyform {
                                opacity,
                                multiply: multiply.map(|(r, g, b)| [r, g, b]),
                                screen: screen.map(|(r, g, b)| [r, g, b]),
                            })
                            .ok_or_else(|| {
                                PyValueError::new_err("Offscreen delta requires opacity")
                            })
                    })
                    .collect::<PyResult<_>>()?,
            ),
        ),
        _ => return Err(PyValueError::new_err("Unknown BlendShape target kind")),
    };
    Ok(BlendShapeBinding {
        id,
        target_id,
        target_kind,
        key_table_id,
        constraint_ids,
        keyforms,
    })
}

pub(crate) fn blend_binding_tuple(value: BlendShapeBinding, version: Version) -> BlendBindingTuple {
    let (kind, forms) = match value.keyforms {
        DeltaKeyforms::Mesh(forms) => (
            "mesh",
            forms
                .into_iter()
                .map(|f| {
                    (
                        f.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                        None,
                        None,
                        None,
                        f.opacity,
                        f.draw_order,
                        None,
                        f.multiply.map(|v| (v[0], v[1], v[2])),
                        f.screen.map(|v| (v[0], v[1], v[2])),
                    )
                })
                .collect(),
        ),
        DeltaKeyforms::Warp(forms) => (
            "warp",
            forms
                .into_iter()
                .map(|f| {
                    (
                        f.points.into_iter().map(|p| (p.x, p.y)).collect(),
                        None,
                        None,
                        None,
                        f.opacity,
                        None,
                        None,
                        f.multiply.map(|v| (v[0], v[1], v[2])),
                        f.screen.map(|v| (v[0], v[1], v[2])),
                    )
                })
                .collect(),
        ),
        DeltaKeyforms::Rotation(forms) => (
            "rotation",
            forms
                .into_iter()
                .map(|f| {
                    (
                        Vec::new(),
                        f.origin.map(|p| (p.x, p.y)),
                        f.angle,
                        f.scale,
                        f.opacity,
                        None,
                        None,
                        f.multiply.map(|v| (v[0], v[1], v[2])),
                        f.screen.map(|v| (v[0], v[1], v[2])),
                    )
                })
                .collect(),
        ),
        DeltaKeyforms::Part(forms) => (
            "part",
            forms
                .into_iter()
                .map(|f| {
                    (
                        Vec::new(),
                        None,
                        None,
                        None,
                        None,
                        Some(f.draw_order),
                        None,
                        None,
                        None,
                    )
                })
                .collect(),
        ),
        DeltaKeyforms::Glue(forms) => (
            "glue",
            forms
                .into_iter()
                .map(|f| {
                    (
                        Vec::new(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(f.intensity),
                        None,
                        None,
                    )
                })
                .collect(),
        ),
        DeltaKeyforms::Offscreen(forms) => (
            "offscreen",
            forms
                .into_iter()
                .map(|f| {
                    (
                        Vec::new(),
                        None,
                        None,
                        None,
                        Some(f.opacity),
                        None,
                        None,
                        f.multiply.map(|v| (v[0], v[1], v[2])),
                        f.screen.map(|v| (v[0], v[1], v[2])),
                    )
                })
                .collect(),
        ),
    };
    (
        value.id,
        value.target_id,
        kind.to_owned(),
        value.key_table_id,
        value.constraint_ids,
        forms,
        version_tuple(version),
    )
}
