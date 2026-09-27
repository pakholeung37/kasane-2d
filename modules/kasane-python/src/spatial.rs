//! Thin CPython bridge for the CPU spatial-query SDK.
use kasane_core::{DrawableFrame, Mesh, Part};
use kasane_sdk::{hit_test_geometry, object_bounds, ObjectTarget, SpatialError};
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct SpatialAuthoring {
    meshes: Vec<Mesh>,
    parts: Vec<Part>,
}

pub(crate) fn targets(raw: Vec<(String, String)>) -> PyResult<Vec<ObjectTarget>> {
    raw.into_iter()
        .map(|(kind, id)| match kind.as_str() {
            "mesh" => Ok(ObjectTarget::Mesh(id)),
            "part" => Ok(ObjectTarget::Part(id)),
            _ => Err(PyValueError::new_err("Object kind must be mesh or part")),
        })
        .collect()
}

pub(crate) fn spatial_error(error: SpatialError) -> PyErr {
    match error {
        SpatialError::MissingMesh(id) => PyKeyError::new_err(format!("Mesh not found: {id}")),
        SpatialError::MissingPart(id) => PyKeyError::new_err(format!("Part not found: {id}")),
        SpatialError::InvalidPoint => PyValueError::new_err("Query point must be finite"),
        SpatialError::InvalidLimit => PyValueError::new_err("max_candidates must be positive"),
    }
}

pub(crate) fn bounds_json(
    meshes: &[Mesh],
    parts: &[Part],
    frame: &DrawableFrame,
    raw_targets: Vec<(String, String)>,
    include_hidden: bool,
) -> PyResult<String> {
    let selected = targets(raw_targets)?;
    let result =
        object_bounds(frame, meshes, parts, &selected, include_hidden).map_err(spatial_error)?;
    serde_json::to_string(&result).map_err(|error| PyValueError::new_err(error.to_string()))
}

pub(crate) fn hit_test_json(
    meshes: &[Mesh],
    parts: &[Part],
    frame: &DrawableFrame,
    canvas_point: (f64, f64),
    include_hidden: bool,
    details: bool,
    max_candidates: usize,
) -> PyResult<String> {
    let result = hit_test_geometry(
        frame,
        meshes,
        parts,
        [canvas_point.0, canvas_point.1],
        include_hidden,
        details,
        max_candidates,
    )
    .map_err(spatial_error)?;
    serde_json::to_string(&result).map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pyclass]
pub(crate) struct NativeSpatialSnapshot {
    meshes: Vec<Mesh>,
    parts: Vec<Part>,
    frame: DrawableFrame,
}

#[pymethods]
impl NativeSpatialSnapshot {
    #[new]
    fn new(authoring_json: &str, evaluated_frame_json: &str) -> PyResult<Self> {
        let authoring: SpatialAuthoring = serde_json::from_str(authoring_json)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let frame: DrawableFrame = serde_json::from_str(evaluated_frame_json)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self {
            meshes: authoring.meshes,
            parts: authoring.parts,
            frame,
        })
    }

    fn bounds_json(
        &self,
        targets: Vec<(String, String)>,
        include_hidden: bool,
    ) -> PyResult<String> {
        bounds_json(
            &self.meshes,
            &self.parts,
            &self.frame,
            targets,
            include_hidden,
        )
    }

    fn hit_test_json(
        &self,
        canvas_point: (f64, f64),
        include_hidden: bool,
        details: bool,
        max_candidates: usize,
    ) -> PyResult<String> {
        hit_test_json(
            &self.meshes,
            &self.parts,
            &self.frame,
            canvas_point,
            include_hidden,
            details,
            max_candidates,
        )
    }
}
