use super::*;
use kasane_core::document::PoseAsset;

#[pymethods]
impl NativeEdit {
    fn set_pose_json(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        self.ensure_open(py, "set_pose_json")?;
        if self
            .commands
            .candidate_document()
            .pose()
            .is_some_and(PoseAsset::has_extensions)
        {
            self.failed = true;
            return Err(edit_failure(
                py,
                "OPAQUE_EDIT_REQUIRES_IMPORT",
                "set_pose_json",
                "Reimport pose3 to edit an asset with unknown fields",
            ));
        }
        let pose: PoseAsset = serde_json::from_str(value).map_err(|error| {
            self.failed = true;
            PyValueError::new_err(error.to_string())
        })?;
        self.commands
            .push(Command::SetPose(pose))
            .map_err(|error| sdk_failure(py, error))
    }

    fn import_pose3(
        &mut self,
        py: Python<'_>,
        id: &str,
        text: &str,
    ) -> PyResult<Vec<(String, String, String)>> {
        self.ensure_open(py, "import_pose3")?;
        let result = self
            .commands
            .workspace
            .as_mut()
            .expect("edit is open")
            .import_pose3(id, text);
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
