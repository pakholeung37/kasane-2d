use super::*;

#[pymethods]
impl NativeSession {
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
}
