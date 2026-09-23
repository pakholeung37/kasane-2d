//! Value conversion at the Python/Rust boundary.
use kasane_core::{
    Appearance, BindingAxis, BlendMode, DrawableFrame, Glue, GlueBinding, GlueKeyform,
    GlueVertexPair, MeshBinding, Offscreen, OffscreenKeyform, PartKeyform, PreciseVec2,
    RotationKeyform, RotationPose, RotationTransform, SceneBinding, SceneKeyform, SceneTrack,
    Transform, TransformData, Vec2, WarpKeyform,
};
use kasane_sdk::{ObjectKind, Version};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

pub(crate) type VersionTuple = (u64, u64, u64);
pub(crate) type PointTuple = (f32, f32);
pub(crate) type ParameterTuple = (String, String, f32, f32, f32, bool, VersionTuple);
pub(crate) type MeshTuple = (String, String, Vec<u32>, Vec<PointTuple>, VersionTuple);
pub(crate) type AssetTuple = (String, String, String, u32, u32, String, VersionTuple);
pub(crate) type EvaluationTuple = (
    Vec<(String, f32, f32, bool)>,
    Vec<(String, Vec<PointTuple>)>,
);
pub(crate) type DiagnosticTuple = (String, String, String);
pub(crate) type ImportTuple = (VersionTuple, u8, Vec<DiagnosticTuple>, Vec<String>);
pub(crate) type BindingForm = (Vec<f32>, Vec<PointTuple>, AppearanceTuple, Option<f32>);
pub(crate) type MeshBindingTuple = (
    String,
    String,
    Vec<(String, Vec<f32>)>,
    Vec<BindingForm>,
    VersionTuple,
);
pub(crate) type DrawOrderTuple = (String, Vec<String>, i32, i32);
pub(crate) type PartTuple = (String, String, String, String, bool, f32, VersionTuple);
pub(crate) type RotationTuple = (f32, (f64, f64, f32, f32, bool, bool));
pub(crate) type WarpTuple = (u32, u32, bool, Vec<PointTuple>);
pub(crate) type TransformTuple = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<RotationTuple>,
    Option<WarpTuple>,
    bool,
    AppearanceTuple,
    VersionTuple,
);
pub(crate) type GeometryTuple = (
    VersionTuple,
    String,
    Vec<u32>,
    Vec<PointTuple>,
    Vec<PointTuple>,
    Vec<(u32, u32, u32)>,
    String,
    Option<String>,
);
pub(crate) type EventTuple = (
    String,
    VersionTuple,
    VersionTuple,
    String,
    Vec<String>,
    bool,
);
pub(crate) type PoseTuple = (f64, f64, f32, f32, bool, bool);
pub(crate) type AppearanceTuple = (f32, Point3Tuple, Point3Tuple);
pub(crate) type Point3Tuple = (f32, f32, f32);
pub(crate) type SceneFormTuple = (
    Vec<f32>,
    Vec<PointTuple>,
    Option<PoseTuple>,
    Option<f32>,
    Option<AppearanceTuple>,
);
pub(crate) type SceneBindingTuple = (
    String,
    Vec<(String, Vec<f32>)>,
    String,
    String,
    Vec<SceneFormTuple>,
    VersionTuple,
);
pub(crate) type MeshPropertiesTuple = (
    String,
    AppearanceTuple,
    Option<f32>,
    String,
    bool,
    bool,
    bool,
    Vec<String>,
    VersionTuple,
);
pub(crate) type OffscreenFormTuple = (f32, Option<Point3Tuple>, Option<Point3Tuple>);
pub(crate) type OffscreenDataTuple = (
    String,
    String,
    String,
    u32,
    u8,
    Vec<String>,
    Vec<i32>,
    Vec<OffscreenFormTuple>,
);
pub(crate) type OffscreenTuple = (
    String,
    String,
    String,
    String,
    u32,
    u8,
    Vec<String>,
    Vec<i32>,
    Vec<OffscreenFormTuple>,
    VersionTuple,
);
pub(crate) type GluePairTuple = (u32, u32, f32, f32);
pub(crate) type GlueBindingTuple = (Vec<(String, Vec<f32>)>, Vec<f32>);
pub(crate) type GlueDataTuple = (
    String,
    String,
    String,
    String,
    Vec<GluePairTuple>,
    f32,
    Option<GlueBindingTuple>,
);
pub(crate) type GlueTuple = (
    String,
    String,
    String,
    String,
    String,
    Vec<GluePairTuple>,
    f32,
    Option<GlueBindingTuple>,
    VersionTuple,
);

pub(crate) fn glue_from_tuple(data: GlueDataTuple, runtime_id: String) -> Glue {
    let (id, name, mesh_a_id, mesh_b_id, pairs, intensity, binding) = data;
    Glue {
        id,
        runtime_id,
        name,
        mesh_a_id,
        mesh_b_id,
        pairs: pairs
            .into_iter()
            .map(|(vertex_a, vertex_b, weight_a, weight_b)| GlueVertexPair {
                vertex_a,
                vertex_b,
                weight_a,
                weight_b,
            })
            .collect(),
        intensity,
        binding: binding.map(|(axes, forms)| GlueBinding {
            axes: axes
                .into_iter()
                .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
                .collect(),
            keyforms: forms
                .into_iter()
                .map(|intensity| GlueKeyform { intensity })
                .collect(),
        }),
    }
}

pub(crate) fn glue_tuple(value: Glue, version: Version) -> GlueTuple {
    (
        value.id,
        value.runtime_id,
        value.name,
        value.mesh_a_id,
        value.mesh_b_id,
        value
            .pairs
            .into_iter()
            .map(|pair| (pair.vertex_a, pair.vertex_b, pair.weight_a, pair.weight_b))
            .collect(),
        value.intensity,
        value.binding.map(|binding| {
            (
                binding
                    .axes
                    .into_iter()
                    .map(|axis| (axis.parameter_id, axis.keys))
                    .collect(),
                binding
                    .keyforms
                    .into_iter()
                    .map(|form| form.intensity)
                    .collect(),
            )
        }),
        version_tuple(version),
    )
}

pub(crate) fn offscreen_from_tuple(data: OffscreenDataTuple, runtime_id: String) -> Offscreen {
    let (id, name, part_id, blend_mode, flags, masks, indices, keyforms) = data;
    Offscreen {
        id,
        runtime_id,
        name,
        part_id,
        blend_mode,
        flags,
        masks,
        part_keyform_indices: indices,
        keyforms: keyforms
            .into_iter()
            .map(|(opacity, multiply, screen)| OffscreenKeyform {
                opacity,
                multiply: multiply.map(|(r, g, b)| [r, g, b]),
                screen: screen.map(|(r, g, b)| [r, g, b]),
            })
            .collect(),
    }
}

pub(crate) fn offscreen_tuple(value: Offscreen, version: Version) -> OffscreenTuple {
    (
        value.id,
        value.runtime_id,
        value.name,
        value.part_id,
        value.blend_mode,
        value.flags,
        value.masks,
        value.part_keyform_indices,
        value
            .keyforms
            .into_iter()
            .map(|form| {
                (
                    form.opacity,
                    form.multiply.map(|rgb| (rgb[0], rgb[1], rgb[2])),
                    form.screen.map(|rgb| (rgb[0], rgb[1], rgb[2])),
                )
            })
            .collect(),
        version_tuple(version),
    )
}

pub(crate) fn blend_mode_from_name(name: &str) -> PyResult<BlendMode> {
    match name {
        "normal" => Ok(BlendMode::Normal),
        "additive" => Ok(BlendMode::Additive),
        "multiplicative" => Ok(BlendMode::Multiplicative),
        _ => Err(PyValueError::new_err("Unknown mesh blend mode")),
    }
}

pub(crate) fn blend_mode_name(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "normal",
        BlendMode::Additive => "additive",
        BlendMode::Multiplicative => "multiplicative",
    }
}

fn pose_from_tuple(value: PoseTuple) -> RotationPose {
    RotationPose {
        origin: PreciseVec2::new(value.0, value.1),
        angle: value.2,
        scale: value.3,
        reflect_x: value.4,
        reflect_y: value.5,
    }
}

fn pose_tuple(pose: RotationPose) -> PoseTuple {
    (
        pose.origin.x,
        pose.origin.y,
        pose.angle,
        pose.scale,
        pose.reflect_x,
        pose.reflect_y,
    )
}

pub(crate) fn appearance_from_tuple(value: AppearanceTuple) -> Appearance {
    let (opacity, multiply, screen) = value;
    Appearance {
        opacity,
        multiply: [multiply.0, multiply.1, multiply.2],
        screen: [screen.0, screen.1, screen.2],
    }
}

pub(crate) fn appearance_tuple(value: Appearance) -> AppearanceTuple {
    (
        value.opacity,
        (value.multiply[0], value.multiply[1], value.multiply[2]),
        (value.screen[0], value.screen[1], value.screen[2]),
    )
}

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

pub(crate) fn version_tuple(version: Version) -> (u64, u64, u64) {
    (version.session_id, version.generation, version.revision)
}

pub(crate) fn tuple_version(value: (u64, u64, u64)) -> Version {
    Version {
        session_id: value.0,
        generation: value.1,
        revision: value.2,
    }
}

pub(crate) fn mesh_tuple(mesh: kasane_core::Mesh, version: Version) -> MeshTuple {
    (
        mesh.id,
        mesh.name,
        mesh.vertex_ids,
        mesh.base_positions
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect(),
        version_tuple(version),
    )
}

pub(crate) fn binding_tuple(binding: MeshBinding, version: Version) -> MeshBindingTuple {
    (
        binding.id,
        binding.mesh_id,
        binding
            .axes
            .into_iter()
            .map(|axis| (axis.parameter_id, axis.keys))
            .collect(),
        binding
            .keyforms
            .into_iter()
            .map(|form| {
                (
                    form.keys,
                    form.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                    appearance_tuple(form.appearance),
                    form.draw_order,
                )
            })
            .collect(),
        version_tuple(version),
    )
}

pub(crate) fn object_kind(value: &str) -> PyResult<ObjectKind> {
    match value {
        "asset" => Ok(ObjectKind::Asset),
        "mesh" => Ok(ObjectKind::Mesh),
        "parameter" => Ok(ObjectKind::Parameter),
        "mesh_binding" => Ok(ObjectKind::MeshBinding),
        "part" => Ok(ObjectKind::Part),
        "transform" => Ok(ObjectKind::Transform),
        "scene_binding" => Ok(ObjectKind::SceneBinding),
        "blend_key_table" => Ok(ObjectKind::BlendKeyTable),
        "blend_constraint" => Ok(ObjectKind::BlendConstraint),
        "blend_binding" => Ok(ObjectKind::BlendBinding),
        "glue" => Ok(ObjectKind::Glue),
        "offscreen" => Ok(ObjectKind::Offscreen),
        _ => Err(PyValueError::new_err(format!(
            "Unknown object kind: {value}"
        ))),
    }
}

pub(crate) fn object_kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Asset => "asset",
        ObjectKind::Mesh => "mesh",
        ObjectKind::Parameter => "parameter",
        ObjectKind::MeshBinding => "mesh_binding",
        ObjectKind::Part => "part",
        ObjectKind::Transform => "transform",
        ObjectKind::SceneBinding => "scene_binding",
        ObjectKind::BlendKeyTable => "blend_key_table",
        ObjectKind::BlendConstraint => "blend_constraint",
        ObjectKind::BlendBinding => "blend_binding",
        ObjectKind::Glue => "glue",
        ObjectKind::Offscreen => "offscreen",
    }
}

pub(crate) fn rotation_data(value: RotationTuple) -> RotationTransform {
    let (base_angle, (x, y, angle, scale, reflect_x, reflect_y)) = value;
    RotationTransform {
        base_angle,
        pose: RotationPose {
            origin: PreciseVec2::new(x, y),
            angle,
            scale,
            reflect_x,
            reflect_y,
        },
    }
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

pub(crate) fn frame_tuple(frame: &DrawableFrame) -> EvaluationTuple {
    (
        frame
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.id.clone(),
                    parameter.requested,
                    parameter.value,
                    parameter.clamped,
                )
            })
            .collect(),
        frame
            .drawables
            .iter()
            .map(|drawable| {
                (
                    drawable.id.clone(),
                    drawable.positions.iter().map(|p| (p.x, p.y)).collect(),
                )
            })
            .collect(),
    )
}
