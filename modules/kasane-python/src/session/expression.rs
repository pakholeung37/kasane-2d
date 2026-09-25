use super::*;
use crate::animation::NativeExpressionPreview;

#[pymethods]
impl NativeSession {
    fn expression_preview(&self, py: Python<'_>) -> PyResult<NativeExpressionPreview> {
        self.ensure_idle(py, "expression_preview")?;
        Ok(NativeExpressionPreview::new(
            self.inner
                .lock()
                .map_err(|_| poisoned())?
                .expression_preview(),
        ))
    }

    fn expression_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .expression_ids()
            .to_vec())
    }

    fn expression_json(&self, id: &str) -> PyResult<Option<String>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .expression(id)
            .map(|expression| {
                serde_json::to_string(&expression)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn export_expression3(&self, py: Python<'_>, id: &str) -> PyResult<String> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_expression3(id)
            .map_err(|error| sdk_failure(py, error))
    }
}
