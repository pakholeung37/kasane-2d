//! CPython bridge: every mutation enters the Rust SDK through a short lock.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kasane_core::{
    draw_order::DrawOrderGroup, Appearance, BindingAxis, Canvas, DrawableFrame, MeshBinding,
    MeshKeyform, Parameter, Vec2,
};
use kasane_sdk::{
    prepare_png_asset, rectangle_mesh, AuthoringSession, EditReceipt, HistoryLimits, ObjectHandle,
    ObjectKind, SdkError, SourceSpace, Version,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

create_exception!(_native, SdkFailure, PyException);

type VersionTuple = (u64, u64, u64);
type PointTuple = (f32, f32);
type ParameterTuple = (String, String, f32, f32, f32, bool, VersionTuple);
type MeshTuple = (String, String, Vec<u32>, Vec<PointTuple>, VersionTuple);
type AssetTuple = (String, String, String, u32, u32, String, VersionTuple);
type EvaluationTuple = (
    Vec<(String, f32, f32, bool)>,
    Vec<(String, Vec<PointTuple>)>,
);
type DiagnosticTuple = (String, String, String);
type ImportTuple = (VersionTuple, u8, Vec<DiagnosticTuple>, Vec<String>);
type BindingForm = (Vec<f32>, Vec<PointTuple>);
type MeshBindingTuple = (
    String,
    String,
    Vec<(String, Vec<f32>)>,
    Vec<BindingForm>,
    VersionTuple,
);
type DrawOrderTuple = (String, Vec<String>, i32, i32);
type GeometryTuple = (
    VersionTuple,
    String,
    Vec<u32>,
    Vec<PointTuple>,
    Vec<PointTuple>,
    Vec<(u32, u32, u32)>,
    String,
    Option<String>,
);
type EventTuple = (
    String,
    VersionTuple,
    VersionTuple,
    String,
    Vec<String>,
    bool,
);

fn sdk_failure(py: Python<'_>, error: SdkError) -> PyErr {
    let failure = SdkFailure::new_err(error.message.to_string());
    let value = failure.value(py);
    let _ = value.setattr("code", error.code.to_string());
    let _ = value.setattr("operation", error.operation);
    let _ = value.setattr("object_ids", error.object_ids);
    let _ = value.setattr("field_path", error.field_path.map(|path| path.to_string()));
    let _ = value.setattr(
        "expected_version",
        error
            .expected_version
            .map(|version| version_tuple(*version)),
    );
    let _ = value.setattr(
        "actual_version",
        error.actual_version.map(|version| version_tuple(*version)),
    );
    let _ = value.setattr("referrers", error.referrers.into_vec());
    failure
}

fn edit_failure(py: Python<'_>, code: &str, operation: &str, message: &str) -> PyErr {
    let failure = SdkFailure::new_err(message.to_owned());
    let value = failure.value(py);
    let _ = value.setattr("code", code);
    let _ = value.setattr("operation", operation);
    let _ = value.setattr("object_ids", Vec::<String>::new());
    let _ = value.setattr("field_path", py.None());
    let _ = value.setattr("expected_version", py.None());
    let _ = value.setattr("actual_version", py.None());
    let _ = value.setattr("referrers", Vec::<String>::new());
    failure
}

fn version_tuple(version: Version) -> (u64, u64, u64) {
    (version.session_id, version.generation, version.revision)
}

fn tuple_version(value: (u64, u64, u64)) -> Version {
    Version {
        session_id: value.0,
        generation: value.1,
        revision: value.2,
    }
}

fn mesh_tuple(mesh: kasane_core::Mesh, version: Version) -> MeshTuple {
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

fn binding_tuple(binding: MeshBinding, version: Version) -> MeshBindingTuple {
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

fn object_kind(value: &str) -> PyResult<ObjectKind> {
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

fn object_kind_name(kind: ObjectKind) -> &'static str {
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

#[pyclass(name = "ObjectHandle")]
struct NativeHandle {
    inner: ObjectHandle,
}

#[pymethods]
impl NativeHandle {
    #[getter]
    fn id(&self) -> &str {
        self.inner.id()
    }

    #[getter]
    fn kind(&self) -> &'static str {
        object_kind_name(self.inner.kind())
    }
}

fn frame_tuple(frame: &DrawableFrame) -> EvaluationTuple {
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

fn poisoned() -> PyErr {
    PyRuntimeError::new_err("Kasane session lock was poisoned")
}

#[pyclass]
struct NativeSession {
    inner: Arc<Mutex<AuthoringSession>>,
}

#[pymethods]
impl NativeSession {
    #[new]
    fn new(
        py: Python<'_>,
        document_id: &str,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
    ) -> PyResult<Self> {
        let canvas = Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        );
        let session =
            AuthoringSession::new(document_id, canvas).map_err(|error| sdk_failure(py, error))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
        })
    }

    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    fn with_history_limits(
        py: Python<'_>,
        document_id: &str,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
        max_steps: usize,
        max_bytes: usize,
    ) -> PyResult<Self> {
        let canvas = Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        );
        let session = AuthoringSession::with_history_limits(
            document_id,
            canvas,
            HistoryLimits {
                max_steps,
                max_bytes,
            },
        )
        .map_err(|error| sdk_failure(py, error))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
        })
    }

    #[staticmethod]
    fn open(py: Python<'_>, path: String) -> PyResult<Self> {
        let result = py.detach(|| {
            let mut session = AuthoringSession::new(
                "00000000-0000-4000-8000-000000000001",
                Canvas::new(1.0, 1.0, Vec2::new(0.0, 0.0), 1.0),
            )?;
            session.open_project(Path::new(&path), None)?;
            Ok::<_, SdkError>(session)
        });
        Ok(Self {
            inner: Arc::new(Mutex::new(result.map_err(|error| sdk_failure(py, error))?)),
        })
    }

    fn version(&self) -> PyResult<(u64, u64, u64)> {
        Ok(version_tuple(
            self.inner.lock().map_err(|_| poisoned())?.version(),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (document_id, width, height, origin_x, origin_y, pixels_per_unit, expected_version=None))]
    fn new_project(
        &self,
        py: Python<'_>,
        document_id: String,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
        expected_version: Option<VersionTuple>,
    ) -> PyResult<VersionTuple> {
        let session = self.inner.clone();
        let canvas = Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        );
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.new_project(
                &document_id,
                canvas,
                expected_version.map(tuple_version),
            ))
        });
        match result {
            Ok(Ok(version)) => Ok(version_tuple(version)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn document_id(&self) -> PyResult<String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .document_id()
            .to_owned())
    }

    fn canvas(&self) -> PyResult<(f32, f32, f32, f32, f32)> {
        let canvas = self.inner.lock().map_err(|_| poisoned())?.canvas();
        Ok((
            canvas.width,
            canvas.height,
            canvas.origin.x,
            canvas.origin.y,
            canvas.pixels_per_unit,
        ))
    }

    fn draw_order_groups(&self) -> PyResult<Option<Vec<DrawOrderTuple>>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .draw_order_groups()
            .map(|groups| {
                groups
                    .into_iter()
                    .map(|g| (g.owner, g.items, g.min_order, g.max_order))
                    .collect()
            }))
    }

    fn evaluation_revision(&self) -> PyResult<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .evaluation_revision())
    }

    fn modified(&self) -> PyResult<bool> {
        Ok(self.inner.lock().map_err(|_| poisoned())?.modified())
    }

    fn project_path(&self) -> PyResult<Option<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .project_path()
            .map(|path| path.to_string_lossy().into_owned()))
    }

    fn mesh_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .mesh_ids()
            .to_vec())
    }

    fn asset_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .asset_ids()
            .to_vec())
    }

    fn parameter_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .parameter_ids()
            .to_vec())
    }

    fn binding_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .binding_ids()
            .to_vec())
    }

    fn part_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .part_ids()
            .to_vec())
    }

    fn transform_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .transform_ids()
            .to_vec())
    }

    fn scene_binding_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .scene_binding_ids()
            .to_vec())
    }

    fn blend_key_table_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .blend_key_table_ids()
            .to_vec())
    }

    fn blend_constraint_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .blend_constraint_ids()
            .to_vec())
    }

    fn blend_binding_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .blend_binding_ids()
            .to_vec())
    }

    fn glue_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .glue_ids()
            .to_vec())
    }

    fn offscreen_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .offscreen_ids()
            .to_vec())
    }

    fn asset(&self, id: &str) -> PyResult<Option<AssetTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.asset(id).map(|asset| {
            (
                asset.id,
                asset.name,
                asset.source,
                asset.width,
                asset.height,
                asset.sha256,
                version_tuple(session.version()),
            )
        }))
    }

    fn references_to(&self, id: &str) -> PyResult<Vec<String>> {
        Ok(self.inner.lock().map_err(|_| poisoned())?.references_to(id))
    }

    fn parameter(&self, id: &str) -> PyResult<Option<ParameterTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.parameter(id).map(|parameter| {
            (
                parameter.id,
                parameter.name,
                parameter.minimum,
                parameter.maximum,
                parameter.default_value,
                parameter.repeat,
                version_tuple(session.version()),
            )
        }))
    }

    fn mesh(&self, id: &str) -> PyResult<Option<MeshTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .mesh(id)
            .map(|mesh| mesh_tuple(mesh, session.version())))
    }

    fn binding(&self, id: &str) -> PyResult<Option<MeshBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .binding(id)
            .map(|binding| binding_tuple(binding, session.version())))
    }

    fn binding_for_mesh(&self, mesh_id: &str) -> PyResult<Option<MeshBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .binding_for_mesh(mesh_id)
            .map(|binding| binding_tuple(binding, session.version())))
    }

    fn handle(&self, py: Python<'_>, kind: &str, id: &str) -> PyResult<NativeHandle> {
        let kind = object_kind(kind)?;
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .handle(kind, id)
            .map(|inner| NativeHandle { inner })
            .map_err(|error| sdk_failure(py, error))
    }

    fn resolve_handle(&self, py: Python<'_>, handle: PyRef<'_, NativeHandle>) -> PyResult<()> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .resolve_handle(&handle.inner)
            .map_err(|error| sdk_failure(py, error))
    }

    fn mesh_by_handle(
        &self,
        py: Python<'_>,
        handle: PyRef<'_, NativeHandle>,
    ) -> PyResult<MeshTuple> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .mesh_by_handle(&handle.inner)
            .map(|mesh| mesh_tuple(mesh, session.version()))
            .map_err(|error| sdk_failure(py, error))
    }

    fn find_meshes_by_name(&self, name: &str) -> PyResult<Vec<MeshTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .find_meshes_by_name(name)
            .into_iter()
            .map(|mesh| mesh_tuple(mesh, session.version()))
            .collect())
    }

    fn require_unique_mesh(&self, py: Python<'_>, name: &str) -> PyResult<MeshTuple> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .require_unique_mesh(name)
            .map(|mesh| mesh_tuple(mesh, session.version()))
            .map_err(|error| sdk_failure(py, error))
    }

    fn geometry(&self, id: &str) -> PyResult<Option<GeometryTuple>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .geometry(id)
            .map(|geometry| {
                let (space, parent) = match geometry.space {
                    SourceSpace::CanvasPixels => ("canvas_pixels".into(), None),
                    SourceSpace::ParentLocal(parent) => ("parent_local".into(), Some(parent)),
                };
                (
                    version_tuple(geometry.version),
                    geometry.mesh_id,
                    geometry.vertex_ids,
                    geometry.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                    geometry.uvs.into_iter().map(|p| (p.x, p.y)).collect(),
                    geometry
                        .triangles
                        .into_iter()
                        .map(|t| (t[0], t[1], t[2]))
                        .collect(),
                    space,
                    parent,
                )
            }))
    }

    fn validate_structure(&self) -> PyResult<Vec<DiagnosticTuple>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .validate_structure()
            .into_iter()
            .map(|issue| (issue.object_id, issue.status.code, issue.status.message))
            .collect())
    }

    fn history_state(&self) -> PyResult<(usize, usize, usize, usize, usize)> {
        let state = self.inner.lock().map_err(|_| poisoned())?.history_state();
        Ok((
            state.undo_steps,
            state.redo_steps,
            state.estimated_bytes,
            state.max_steps,
            state.max_bytes,
        ))
    }

    fn history_lengths(&self) -> PyResult<(usize, usize)> {
        Ok(self.inner.lock().map_err(|_| poisoned())?.history_lengths())
    }

    fn estimated_content_bytes(&self) -> PyResult<usize> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .estimated_content_bytes())
    }

    fn drain_events(&self) -> PyResult<Vec<EventTuple>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .drain_events()
            .into_iter()
            .map(|receipt| {
                (
                    receipt.label,
                    version_tuple(receipt.before),
                    version_tuple(receipt.after),
                    format!("{:?}", receipt.kind),
                    receipt.object_ids,
                    receipt.changed,
                )
            })
            .collect())
    }

    fn diagnose_resources(&self) -> PyResult<Vec<(String, String, String)>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .diagnose_resources()
            .into_iter()
            .map(|item| (item.asset_id, item.code, item.message))
            .collect())
    }

    fn evaluate(&self, py: Python<'_>, values: HashMap<String, f32>) -> PyResult<EvaluationTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.evaluate(&values))
        });
        match result {
            Ok(Ok(frame)) => Ok(frame_tuple(&frame)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn preview_values(&self) -> PyResult<HashMap<String, f32>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .preview_values()
            .clone())
    }

    fn preview_revision(&self) -> PyResult<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .preview_revision())
    }

    fn preview_evaluation_count(&self) -> PyResult<u64> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .preview_evaluation_count())
    }

    fn preview_frame(&self, py: Python<'_>) -> PyResult<EvaluationTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.preview_frame().map(|frame| frame_tuple(&frame)))
        });
        match result {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn set_preview_values(&self, py: Python<'_>, values: HashMap<String, f32>) -> PyResult<bool> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.set_preview_values(values))
        });
        match result {
            Ok(Ok(changed)) => Ok(changed),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn set_preview_parameter(&self, py: Python<'_>, id: String, value: f32) -> PyResult<bool> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.set_preview_parameter(&id, value))
        });
        match result {
            Ok(Ok(changed)) => Ok(changed),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn reset_preview_values(&self, py: Python<'_>) -> PyResult<bool> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.reset_preview_values())
        });
        match result {
            Ok(Ok(changed)) => Ok(changed),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (path, expected_version=None))]
    fn import_model3(
        &self,
        py: Python<'_>,
        path: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<ImportTuple> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(
                session.import_model3(Path::new(&path), expected_version.map(tuple_version)),
            )
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                version_tuple(receipt.after),
                receipt.report.moc_version,
                receipt
                    .project
                    .diagnostics
                    .into_iter()
                    .map(|d| (d.asset_id, d.code, d.message))
                    .collect(),
                receipt.project.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (path, texture_map, expected_version=None))]
    fn import_bare_moc3(
        &self,
        py: Python<'_>,
        path: String,
        texture_map: HashMap<usize, String>,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<ImportTuple> {
        let texture_map: HashMap<_, _> = texture_map
            .into_iter()
            .map(|(slot, path)| (slot, PathBuf::from(path)))
            .collect();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.import_bare_moc3(
                Path::new(&path),
                &texture_map,
                expected_version.map(tuple_version),
            ))
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                version_tuple(receipt.after),
                receipt.report.moc_version,
                receipt
                    .project
                    .diagnostics
                    .into_iter()
                    .map(|d| (d.asset_id, d.code, d.message))
                    .collect(),
                receipt.project.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (destination, expected_version=None))]
    fn export_package(
        &self,
        py: Python<'_>,
        destination: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<(bool, bool, Vec<String>)> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(
                session
                    .export_package(Path::new(&destination), expected_version.map(tuple_version)),
            )
        });
        match result {
            Ok(Ok(publication)) => Ok((
                publication.published,
                publication.durable,
                publication.warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    #[pyo3(signature = (label, expected_version=None))]
    fn start_edit(
        &self,
        label: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<NativeEdit> {
        let current = self.inner.lock().map_err(|_| poisoned())?.version();
        Ok(NativeEdit {
            session: self.inner.clone(),
            label,
            expected: expected_version.map(tuple_version).unwrap_or(current),
            commands: Vec::new(),
            failed: false,
            closed: false,
        })
    }

    #[pyo3(signature = (path, expected_version=None))]
    fn save(
        &self,
        py: Python<'_>,
        path: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<(String, bool, Vec<String>, Vec<String>)> {
        let session = self.inner.clone();
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.save_project(Path::new(&path), expected_version.map(tuple_version)))
        });
        match result {
            Ok(Ok(receipt)) => Ok((
                receipt.manifest.to_string_lossy().into_owned(),
                receipt.durable,
                receipt.warnings,
                receipt.history_warnings,
            )),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn undo(&self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        let mut session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .undo()
            .map(|receipt| version_tuple(receipt.after))
            .map_err(|error| sdk_failure(py, error))
    }

    fn redo(&self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        let mut session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .redo()
            .map(|receipt| version_tuple(receipt.after))
            .map_err(|error| sdk_failure(py, error))
    }
}

enum Command {
    AddPng(kasane_core::ImageAsset),
    CreateRectangle(Box<kasane_core::Mesh>),
    RenameMesh(String, String),
    UpdatePositions(String, Vec<u32>, Vec<Vec2>),
    CreateParameter(Parameter),
    CreateMeshBinding(MeshBinding),
    ReplaceCanvas(Canvas),
    EraseObject(String),
    ReplaceParameter(Parameter),
    SetOrganizationParent(String, String),
    SetTransformParent(String, Option<String>),
    SetTransformPart(String, Option<String>),
    SetDeformParent(String, String),
    SetMeshPart(String, String),
    ReplaceDrawOrderGroups(Vec<DrawOrderGroup>),
}

#[pyclass]
struct NativeEdit {
    session: Arc<Mutex<AuthoringSession>>,
    label: String,
    expected: Version,
    commands: Vec<Command>,
    failed: bool,
    closed: bool,
}

impl NativeEdit {
    fn ensure_open(&self, py: Python<'_>, operation: &str) -> PyResult<()> {
        if self.closed {
            Err(edit_failure(
                py,
                "EDIT_CLOSED",
                operation,
                "Edit is already closed",
            ))
        } else if self.failed {
            Err(edit_failure(
                py,
                "EDIT_ABORTED",
                operation,
                "Edit was aborted by a previous error",
            ))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl NativeEdit {
    fn replace_draw_order_groups(
        &mut self,
        py: Python<'_>,
        groups: Vec<DrawOrderTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_draw_order_groups")?;
        self.commands.push(Command::ReplaceDrawOrderGroups(
            groups
                .into_iter()
                .map(|(owner, items, min_order, max_order)| DrawOrderGroup {
                    owner,
                    items,
                    min_order,
                    max_order,
                })
                .collect(),
        ));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_canvas(
        &mut self,
        py: Python<'_>,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_canvas")?;
        self.commands.push(Command::ReplaceCanvas(Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        )));
        Ok(())
    }

    fn erase_object(&mut self, py: Python<'_>, id: String) -> PyResult<()> {
        self.ensure_open(py, "erase_object")?;
        self.commands.push(Command::EraseObject(id));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (id, name, minimum, maximum, default_value, repeat=false))]
    fn replace_parameter(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        repeat: bool,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_parameter")?;
        let original = self.session.lock().map_err(|_| poisoned())?.parameter(&id);
        let mut parameter = original.unwrap_or_else(|| Parameter {
            runtime_id: id.clone(),
            ..Parameter::default()
        });
        parameter.id = id;
        parameter.name = name;
        parameter.minimum = minimum;
        parameter.maximum = maximum;
        parameter.default_value = default_value;
        parameter.repeat = repeat;
        self.commands.push(Command::ReplaceParameter(parameter));
        Ok(())
    }

    fn set_organization_parent(
        &mut self,
        py: Python<'_>,
        part_id: String,
        parent_id: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_organization_parent")?;
        self.commands
            .push(Command::SetOrganizationParent(part_id, parent_id));
        Ok(())
    }

    fn set_transform_parent(
        &mut self,
        py: Python<'_>,
        transform_id: String,
        parent_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_transform_parent")?;
        self.commands
            .push(Command::SetTransformParent(transform_id, parent_id));
        Ok(())
    }

    fn set_transform_part(
        &mut self,
        py: Python<'_>,
        transform_id: String,
        part_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_transform_part")?;
        self.commands
            .push(Command::SetTransformPart(transform_id, part_id));
        Ok(())
    }

    fn set_deform_parent(
        &mut self,
        py: Python<'_>,
        mesh_id: String,
        transform_id: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_deform_parent")?;
        self.commands
            .push(Command::SetDeformParent(mesh_id, transform_id));
        Ok(())
    }

    fn set_mesh_part(&mut self, py: Python<'_>, mesh_id: String, part_id: String) -> PyResult<()> {
        self.ensure_open(py, "set_mesh_part")?;
        self.commands.push(Command::SetMeshPart(mesh_id, part_id));
        Ok(())
    }

    fn add_png_asset(&mut self, py: Python<'_>, id: &str, name: &str, path: &str) -> PyResult<()> {
        self.ensure_open(py, "add_png_asset")?;
        let id = id.to_owned();
        let name = name.to_owned();
        let path = path.to_owned();
        match py.detach(move || prepare_png_asset(&id, &name, Path::new(&path))) {
            Ok(asset) => {
                self.commands.push(Command::AddPng(asset));
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn create_rectangle(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        asset_id: &str,
        minimum: (f32, f32),
        maximum: (f32, f32),
    ) -> PyResult<()> {
        self.ensure_open(py, "create_rectangle")?;
        match rectangle_mesh(
            id,
            name,
            asset_id,
            Vec2::new(minimum.0, minimum.1),
            Vec2::new(maximum.0, maximum.1),
        ) {
            Ok(mesh) => {
                self.commands.push(Command::CreateRectangle(Box::new(mesh)));
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn rename_mesh(&mut self, py: Python<'_>, id: String, name: String) -> PyResult<()> {
        self.ensure_open(py, "rename_mesh")?;
        self.commands.push(Command::RenameMesh(id, name));
        Ok(())
    }

    fn update_positions(
        &mut self,
        py: Python<'_>,
        id: String,
        vertex_ids: Vec<u32>,
        positions: Vec<(f32, f32)>,
    ) -> PyResult<()> {
        self.ensure_open(py, "update_positions")?;
        self.commands.push(Command::UpdatePositions(
            id,
            vertex_ids,
            positions
                .into_iter()
                .map(|(x, y)| Vec2::new(x, y))
                .collect(),
        ));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (id, name, minimum, maximum, default_value, repeat=false))]
    fn create_parameter(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        repeat: bool,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_parameter")?;
        self.commands.push(Command::CreateParameter(Parameter {
            id,
            name,
            minimum,
            maximum,
            default_value,
            repeat,
            ..Parameter::default()
        }));
        Ok(())
    }

    fn create_mesh_binding(
        &mut self,
        py: Python<'_>,
        id: String,
        mesh_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<BindingForm>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_mesh_binding")?;
        self.commands.push(Command::CreateMeshBinding(MeshBinding {
            id,
            mesh_id,
            axes: axes
                .into_iter()
                .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
                .collect(),
            keyforms: forms
                .into_iter()
                .map(|(keys, positions)| MeshKeyform {
                    keys,
                    positions: positions
                        .into_iter()
                        .map(|(x, y)| Vec2::new(x, y))
                        .collect(),
                    appearance: Appearance::default(),
                    draw_order: None,
                })
                .collect(),
        }));
        Ok(())
    }

    fn commit(&mut self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        self.ensure_open(py, "commit")?;
        self.closed = true;
        let session = self.session.clone();
        let label = self.label.clone();
        let expected = self.expected;
        let commands = std::mem::take(&mut self.commands);
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.edit(&label, Some(expected), |edit| {
                for command in commands {
                    match command {
                        Command::AddPng(asset) => edit.create_asset(asset)?,
                        Command::CreateRectangle(mesh) => edit.create_mesh(*mesh)?,
                        Command::RenameMesh(id, name) => edit.rename_mesh(&id, name)?,
                        Command::UpdatePositions(id, ids, positions) => {
                            edit.update_positions(&id, &ids, &positions)?
                        }
                        Command::CreateParameter(parameter) => edit.create_parameter(parameter)?,
                        Command::CreateMeshBinding(binding) => edit.create_binding(binding)?,
                        Command::ReplaceCanvas(canvas) => edit.replace_canvas(canvas)?,
                        Command::EraseObject(id) => edit.erase_object(&id)?,
                        Command::ReplaceParameter(parameter) => {
                            edit.replace_parameter(parameter)?
                        }
                        Command::SetOrganizationParent(id, parent) => {
                            edit.set_organization_parent(&id, &parent)?
                        }
                        Command::SetTransformParent(id, parent) => {
                            edit.set_transform_parent(&id, parent.map(Into::into))?
                        }
                        Command::SetTransformPart(id, part) => {
                            edit.set_transform_part(&id, part.map(Into::into))?
                        }
                        Command::SetDeformParent(id, parent) => {
                            edit.set_deform_parent(&id, &parent)?
                        }
                        Command::SetMeshPart(id, part) => edit.set_mesh_part(&id, &part)?,
                        Command::ReplaceDrawOrderGroups(groups) => {
                            edit.replace_draw_order_groups(groups)?
                        }
                    }
                }
                Ok(())
            }))
        });
        match result {
            Ok(Ok(((), EditReceipt { after, .. }))) => Ok(version_tuple(after)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn cancel(&mut self) {
        self.closed = true;
        self.commands.clear();
    }

    fn abort(&mut self) {
        self.failed = true;
        self.commands.clear();
    }
}

#[pyfunction]
fn capabilities(py: Python<'_>) -> PyResult<Py<PyDict>> {
    let result = PyDict::new(py);
    result.set_item("project_io", true)?;
    result.set_item("model3_import", true)?;
    result.set_item("bare_moc3_import", true)?;
    result.set_item("moc3_export", true)?;
    result.set_item("official_core_validation", kasane_moc3::HAS_CORE_VALIDATION)?;
    result.set_item("gpu_observation", false)?;
    Ok(result.unbind())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("SdkFailure", module.py().get_type::<SdkFailure>())?;
    module.add_class::<NativeSession>()?;
    module.add_class::<NativeEdit>()?;
    module.add_class::<NativeHandle>()?;
    module.add_function(wrap_pyfunction!(capabilities, module)?)?;
    Ok(())
}
