//! Geometry adapters; algorithms belong to the Rust SDK.
use crate::conversion::MeshGeometryDataTuple;
use crate::error::sdk_failure;
use kasane_core::Vec2;
use pyo3::prelude::*;

pub(crate) fn grid_error(py: Python<'_>, error: kasane_sdk::SdkError) -> PyErr {
    // Preserve the existing Python recipe's validation exception contract.
    if error.code.as_ref() == "INVALID_RECTANGLE_GRID" || error.code.as_ref() == "NOT_FOUND" {
        pyo3::exceptions::PyValueError::new_err(error.message.to_string())
    } else {
        sdk_failure(py, error)
    }
}

#[pyfunction]
pub(crate) fn rectangle_grid_geometry(
    py: Python<'_>,
    source: MeshGeometryDataTuple,
    columns: usize,
    rows: usize,
) -> PyResult<MeshGeometryDataTuple> {
    let (vertex_ids, positions, uvs, triangles) = source;
    let geometry = kasane_sdk::rectangle_grid_geometry(
        &kasane_sdk::MeshGeometry {
            vertex_ids,
            positions: positions
                .into_iter()
                .map(|(x, y)| Vec2::new(x, y))
                .collect(),
            uvs: uvs.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
            triangles: triangles.into_iter().map(|(a, b, c)| [a, b, c]).collect(),
        },
        columns,
        rows,
    )
    .map_err(|e| grid_error(py, e))?;
    Ok((
        geometry.vertex_ids,
        geometry.positions.into_iter().map(|p| (p.x, p.y)).collect(),
        geometry.uvs.into_iter().map(|p| (p.x, p.y)).collect(),
        geometry
            .triangles
            .into_iter()
            .map(|[a, b, c]| (a, b, c))
            .collect(),
    ))
}
