use super::*;
use kasane_core::document::{Model3Settings, PackageAttachment};

#[pymethods]
impl NativeEdit {
    fn set_package_attachments(
        &mut self,
        py: Python<'_>,
        attachments: Vec<(String, Vec<u8>)>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_package_attachments")?;
        self.commands
            .push(Command::SetPackageAttachments(
                attachments
                    .into_iter()
                    .map(|(path, bytes)| PackageAttachment { path, bytes })
                    .collect(),
            ))
            .map_err(|error| sdk_failure(py, error))
    }
    fn set_model3_settings_json(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        self.ensure_open(py, "set_model3_settings_json")?;
        let settings: Model3Settings = serde_json::from_str(value).map_err(|error| {
            self.failed = true;
            PyValueError::new_err(error.to_string())
        })?;
        self.commands
            .push(Command::SetModel3Settings(settings))
            .map_err(|error| sdk_failure(py, error))
    }
}
