use super::*;
use kasane_core::document::PhysicsAsset;

#[pymethods]
impl NativeEdit {
    fn discard_missing_attachment(&mut self, py: Python<'_>, path: &str) -> PyResult<()> {
        self.ensure_open(py, "discard_missing_attachment")?;
        let result = self
            .commands
            .workspace
            .as_mut()
            .expect("edit is open")
            .discard_missing_attachment(path);
        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                self.commands.error = Some(error.clone());
                Err(sdk_failure(py, error))
            }
        }
    }
    fn set_physics_json(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        self.ensure_open(py, "set_physics_json")?;
        if self
            .commands
            .candidate_document()
            .physics()
            .is_some_and(|asset| asset.opaque_source_ids.is_some())
        {
            self.failed = true;
            return Err(edit_failure(
                py,
                "OPAQUE_EDIT_REQUIRES_IMPORT",
                "set_physics_json",
                "Reimport physics3 to edit an asset with unknown fields",
            ));
        }
        let asset: PhysicsAsset = serde_json::from_str(value).map_err(|error| {
            self.failed = true;
            PyValueError::new_err(error.to_string())
        })?;
        self.commands
            .push(Command::SetPhysics(asset))
            .map_err(|error| sdk_failure(py, error))
    }

    fn import_physics3(
        &mut self,
        py: Python<'_>,
        id: &str,
        text: &str,
    ) -> PyResult<Vec<(String, String, String)>> {
        self.ensure_open(py, "import_physics3")?;
        let result = self
            .commands
            .workspace
            .as_mut()
            .expect("edit is open")
            .import_physics3(id, text);
        match result {
            Ok(diagnostics) => Ok(diagnostics
                .into_iter()
                .map(|item| (item.code, item.path, item.message))
                .collect()),
            Err(error) => {
                self.commands.error = Some(error.clone());
                Err(sdk_failure(py, error))
            }
        }
    }
}
