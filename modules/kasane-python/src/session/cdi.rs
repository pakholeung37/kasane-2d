use super::*;

#[pymethods]
impl NativeSession {
    fn display_info_json(&self) -> PyResult<String> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        serde_json::to_string(&session.display_info())
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    fn export_cdi3(&self, py: Python<'_>) -> PyResult<String> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_cdi3()
            .map_err(|error| sdk_failure(py, error))
    }
}
