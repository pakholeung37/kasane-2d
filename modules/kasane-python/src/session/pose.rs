use super::*;

#[pymethods]
impl NativeSession {
    fn pose_json(&self) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .pose()
            .map(|pose| {
                serde_json::to_string(&pose)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn export_pose3(&self, py: Python<'_>) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_pose3()
            .map_err(|error| sdk_failure(py, error))
    }
}
