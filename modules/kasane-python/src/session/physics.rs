use super::*;
use crate::animation::NativePhysicsPreview;

#[pymethods]
impl NativeSession {
    fn physics_preview(&self, py: Python<'_>) -> PyResult<NativePhysicsPreview> {
        self.ensure_idle(py, "physics_preview")?;
        Ok(NativePhysicsPreview::new(
            self.inner.lock().map_err(|_| poisoned())?.physics_preview(),
        ))
    }
    fn missing_attachments(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .missing_attachments())
    }
    fn physics_json(&self) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .physics()
            .map(|asset| {
                serde_json::to_string(&asset)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn export_physics3(&self, py: Python<'_>) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_physics3()
            .map_err(|error| sdk_failure(py, error))
    }
}
