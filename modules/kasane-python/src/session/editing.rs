use super::*;

#[pymethods]
impl NativeSession {
    #[pyo3(signature = (label, expected_version=None))]
    fn start_edit(
        &self,
        py: Python<'_>,
        label: String,
        expected_version: Option<(u64, u64, u64)>,
    ) -> PyResult<NativeEdit> {
        let locked = self.inner.lock().map_err(|_| poisoned())?;
        if self.active_edit.load(Ordering::Acquire) {
            return Err(edit_failure(
                py,
                "EDIT_ACTIVE",
                "start_edit",
                "An edit is active",
            ));
        }
        let workspace = locked
            .begin_owned_edit(label, expected_version.map(tuple_version))
            .map_err(|error| sdk_failure(py, error))?;
        self.active_edit.store(true, Ordering::Release);
        Ok(NativeEdit::new(
            self.inner.clone(),
            self.active_edit.clone(),
            workspace,
        ))
    }

    fn undo(&self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, "undo")?;
        let mut session = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, "undo")?;
        session
            .undo()
            .map(|receipt| version_tuple(receipt.after))
            .map_err(|error| sdk_failure(py, error))
    }

    fn redo(&self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, "redo")?;
        let mut session = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, "redo")?;
        session
            .redo()
            .map(|receipt| version_tuple(receipt.after))
            .map_err(|error| sdk_failure(py, error))
    }
}
