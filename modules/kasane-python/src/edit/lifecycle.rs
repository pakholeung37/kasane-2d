use super::*;

#[pymethods]
impl NativeEdit {
    fn parameter(&self, py: Python<'_>, id: &str) -> PyResult<Option<ParameterTuple>> {
        self.ensure_open(py, "parameter")?;
        let workspace = self.commands.workspace.as_ref().expect("edit is open");
        Ok(workspace.candidate_document().get_parameter(id).map(|p| {
            (
                p.id.clone(),
                p.name.clone(),
                p.minimum,
                p.maximum,
                p.default_value,
                p.repeat,
                parameter_kind_name(p.kind).to_owned(),
                version_tuple(workspace.base_version()),
            )
        }))
    }

    fn mesh(&self, py: Python<'_>, id: &str) -> PyResult<Option<MeshTuple>> {
        self.ensure_open(py, "mesh")?;
        let workspace = self.commands.workspace.as_ref().expect("edit is open");
        Ok(workspace
            .candidate_document()
            .get_mesh(id)
            .cloned()
            .map(|mesh| mesh_tuple(mesh, workspace.base_version())))
    }

    fn commit(&mut self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        if let Some(error) = self.commands.error.take() {
            self.closed = true;
            self.commands.clear();
            self.release_active();
            return Err(sdk_failure(py, error));
        }
        if let Err(error) = self.ensure_open(py, "commit") {
            self.closed = true;
            self.commands.clear();
            self.release_active();
            return Err(error);
        }
        self.closed = true;
        let session = self.session.clone();
        let workspace = self.commands.workspace.take().expect("edit is open");
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(workspace.commit_to(&mut session))
        });
        self.release_active();
        match result {
            Ok(Ok(receipt)) => Ok(version_tuple(receipt.after)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn cancel(&mut self) {
        self.closed = true;
        self.commands.clear();
        self.release_active();
    }

    fn abort(&mut self) {
        self.failed = true;
        self.commands.clear();
        self.release_active();
    }
}
