//! Value conversion at the Python/Rust boundary.
use kasane_core::{
    DrawableFrame, MeshBinding, PreciseVec2, RotationPose, RotationTransform, Transform,
    TransformData,
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
pub(crate) type BindingForm = (Vec<f32>, Vec<PointTuple>);
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
