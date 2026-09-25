use super::*;
use crate::animation::NativeMotionPreview;

#[pymethods]
impl NativeSession {
    fn motion_preview(&self, py: Python<'_>) -> PyResult<NativeMotionPreview> {
        self.ensure_idle(py, "motion_preview")?;
        Ok(NativeMotionPreview::new(
            self.inner.lock().map_err(|_| poisoned())?.motion_preview(),
        ))
    }
    fn motion_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .motion_ids()
            .to_vec())
    }

    fn motion_json(&self, id: &str) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .motion(id)
            .map(|clip| {
                serde_json::to_string(&clip)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn motion_groups_json(&self) -> PyResult<String> {
        let groups = self.inner.lock().map_err(|_| poisoned())?.motion_groups();
        serde_json::to_string(&groups)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    fn export_motion3(&self, py: Python<'_>, id: &str) -> PyResult<String> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_motion3(id)
            .map_err(|error| sdk_failure(py, error))
    }
}
