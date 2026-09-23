//! Python-facing authoring session and read/IO operations.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::conversion::*;
use crate::edit::NativeEdit;
use crate::error::{poisoned, sdk_failure};
use crate::handle::NativeHandle;
use kasane_core::{Canvas, Vec2};
use kasane_sdk::{
    AuthoringSession, GeometryBounds, GeometryChecks, GeometryDiagnosticKind, HistoryLimits,
    MeshProperties, SdkError, SourceSpace,
};
use pyo3::prelude::*;

#[pyclass]
pub(crate) struct NativeSession {
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

    fn mesh_properties(&self, id: &str) -> PyResult<Option<MeshPropertiesTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.mesh(id).map(|mesh| {
            let props = MeshProperties::from(&mesh);
            (
                props.texture_asset_id,
                appearance_tuple(props.appearance),
                props.draw_order,
                blend_mode_name(props.blend_mode).into(),
                props.enabled,
                props.double_sided,
                props.inverted_mask,
                props.masks,
                version_tuple(session.version()),
            )
        }))
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

    fn part(&self, id: &str) -> PyResult<Option<PartTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.part(id).map(|part| {
            (
                part.id,
                part.runtime_id,
                part.name,
                part.parent_id,
                part.enabled,
                part.draw_order,
                version_tuple(session.version()),
            )
        }))
    }

    fn transform(&self, id: &str) -> PyResult<Option<TransformTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .transform(id)
            .map(|transform| transform_tuple(transform, session.version())))
    }

    fn offscreen(&self, id: &str) -> PyResult<Option<OffscreenTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .offscreen(id)
            .map(|value| offscreen_tuple(value, session.version())))
    }

    fn scene_binding(&self, id: &str) -> PyResult<Option<SceneBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .scene_binding(id)
            .map(|binding| scene_binding_tuple(binding, session.version())))
    }

    fn binding_for_scene(&self, target_id: &str) -> PyResult<Option<SceneBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .binding_for_scene(target_id)
            .map(|binding| scene_binding_tuple(binding, session.version())))
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

    #[pyo3(signature = (min_triangle_area=0.0, canvas_bounds=None))]
    fn diagnose_geometry(
        &self,
        py: Python<'_>,
        min_triangle_area: f64,
        canvas_bounds: Option<(PointTuple, PointTuple)>,
    ) -> PyResult<Vec<(String, String, Option<usize>)>> {
        let session = self.inner.clone();
        let checks = GeometryChecks {
            min_triangle_area,
            canvas_bounds: canvas_bounds.map(|(min, max)| GeometryBounds {
                min: Vec2::new(min.0, min.1),
                max: Vec2::new(max.0, max.1),
            }),
        };
        let result = py.detach(move || {
            let session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.diagnose_geometry(checks))
        });
        match result {
            Ok(Ok(issues)) => Ok(issues
                .into_iter()
                .map(|issue| {
                    let kind = match issue.kind {
                        GeometryDiagnosticKind::SmallTriangle => "small_triangle",
                        GeometryDiagnosticKind::InconsistentWinding => "inconsistent_winding",
                        GeometryDiagnosticKind::OutsideCanvasBounds => "outside_canvas_bounds",
                    };
                    (kind.into(), issue.mesh_id, issue.triangle_index)
                })
                .collect()),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
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
        Ok(NativeEdit::new(
            self.inner.clone(),
            label,
            expected_version.map(tuple_version).unwrap_or(current),
        ))
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
