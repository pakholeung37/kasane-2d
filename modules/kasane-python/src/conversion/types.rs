use super::*;

pub(crate) type VersionTuple = (u64, u64, u64);
pub(crate) type PointTuple = (f32, f32);
pub(crate) type ParameterTuple = (String, String, f32, f32, f32, bool, String, VersionTuple);
pub(crate) type MeshTuple = (String, String, Vec<u32>, Vec<PointTuple>, VersionTuple);
pub(crate) type AssetTuple = (String, String, String, u32, u32, String, VersionTuple);
pub(crate) type EvaluationTuple = (
    Vec<(String, f32, f32, bool)>,
    Vec<(String, Vec<PointTuple>)>,
);
pub(crate) type FullDrawableTuple = (
    (
        String,
        String,
        String,
        Option<u32>,
        String,
        i32,
        Vec<PointTuple>,
        Vec<PointTuple>,
        Vec<u32>,
        i32,
    ),
    (
        i32,
        f32,
        [f32; 4],
        [f32; 4],
        String,
        bool,
        bool,
        bool,
        bool,
        Vec<String>,
    ),
);
pub(crate) type FullOffscreenTuple = (
    String,
    String,
    String,
    Option<String>,
    i32,
    f32,
    bool,
    u32,
    u8,
    Vec<String>,
    [f32; 4],
    [f32; 4],
);
pub(crate) type FullEvaluationTuple = (
    VersionTuple,
    u64,
    (f32, f32, f32, f32, f32),
    Vec<(String, f32, f32, bool)>,
    Vec<FullDrawableTuple>,
    Vec<FullOffscreenTuple>,
    Vec<(String, String)>,
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
pub(crate) type MeshBindingDataTuple = (String, String, Vec<(String, Vec<f32>)>, Vec<BindingForm>);

pub(crate) fn mesh_binding_from_tuple(data: MeshBindingDataTuple) -> MeshBinding {
    let (id, mesh_id, axes, forms) = data;
    MeshBinding {
        id,
        mesh_id,
        axes: axes
            .into_iter()
            .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
            .collect(),
        keyforms: forms
            .into_iter()
            .map(|(keys, positions, appearance, draw_order)| MeshKeyform {
                keys,
                positions: positions
                    .into_iter()
                    .map(|(x, y)| Vec2::new(x, y))
                    .collect(),
                appearance: appearance_from_tuple(appearance),
                draw_order,
            })
            .collect(),
    }
}

pub(crate) fn geometry_from_tuple(value: GeometryTuple) -> PyResult<GeometrySnapshot> {
    let (version, mesh_id, vertex_ids, positions, uvs, triangles, space, parent) = value;
    let space = match (space.as_str(), parent) {
        ("canvas_pixels", None) => SourceSpace::CanvasPixels,
        ("parent_local", Some(parent)) => SourceSpace::ParentLocal(parent),
        _ => return Err(PyValueError::new_err("Invalid geometry source space")),
    };
    Ok(GeometrySnapshot {
        version: tuple_version(version),
        mesh_id,
        vertex_ids,
        positions: positions
            .into_iter()
            .map(|(x, y)| Vec2::new(x, y))
            .collect(),
        uvs: uvs.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
        triangles: triangles.into_iter().map(|(a, b, c)| [a, b, c]).collect(),
        space,
    })
}
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
pub(crate) type BlendKeyTableTuple = (String, String, Vec<f32>, usize, VersionTuple);
pub(crate) type BlendConstraintTuple = (String, String, Vec<f32>, Vec<f32>, VersionTuple);
pub(crate) type BlendFormTuple = (
    Vec<PointTuple>,
    Option<PointTuple>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<Point3Tuple>,
    Option<Point3Tuple>,
);
pub(crate) type BlendBindingDataTuple = (
    String,
    String,
    String,
    String,
    Vec<String>,
    Vec<BlendFormTuple>,
);
pub(crate) type BlendBindingTuple = (
    String,
    String,
    String,
    String,
    Vec<String>,
    Vec<BlendFormTuple>,
    VersionTuple,
);
pub(crate) type MeshGeometryDataTuple = (
    Vec<u32>,
    Vec<PointTuple>,
    Vec<PointTuple>,
    Vec<(u32, u32, u32)>,
);
pub(crate) type MeshDrawingDataTuple = (
    Option<f32>,
    String,
    bool,
    bool,
    bool,
    Vec<String>,
    Option<u32>,
);
pub(crate) type MeshRecordDataTuple = (
    String,
    String,
    String,
    MeshGeometryDataTuple,
    (String, String),
    AppearanceTuple,
    MeshDrawingDataTuple,
);
pub(crate) type MeshRecordTuple = (MeshRecordDataTuple, String, VersionTuple);
