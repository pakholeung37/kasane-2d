use super::*;
use kasane_core::document::{ExpressionAsset, ExpressionBlend, ExpressionEntry, ExpressionTarget};

fn blend(value: &str) -> PyResult<Option<ExpressionBlend>> {
    match value {
        "add" => Ok(Some(ExpressionBlend::Add)),
        "multiply" => Ok(Some(ExpressionBlend::Multiply)),
        "overwrite" => Ok(Some(ExpressionBlend::Overwrite)),
        "default" => Ok(None),
        _ => Err(PyValueError::new_err(
            "blend must be add, multiply, overwrite, or default",
        )),
    }
}

#[pymethods]
impl NativeEdit {
    #[pyo3(signature = (id, name, entries, fade_in=None, fade_out=None))]
    fn create_expression(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        entries: Vec<(String, f32, String)>,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_expression")?;
        let mut parameters = Vec::with_capacity(entries.len());
        for (parameter_id, value, mode) in entries {
            let blend = match blend(&mode) {
                Ok(blend) => blend,
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            };
            parameters.push(ExpressionEntry {
                target: ExpressionTarget::Resolved { parameter_id },
                value,
                blend,
                extensions: Default::default(),
            });
        }
        self.commands
            .push(Command::CreateExpression(ExpressionAsset {
                id,
                name,
                file_type: Some("Live2D Expression".into()),
                fade_in,
                fade_out,
                entries: parameters,
                extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            }))
            .map_err(|error| sdk_failure(py, error))
    }

    #[pyo3(signature = (id, name, entries, fade_in=None, fade_out=None))]
    fn replace_expression(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        entries: Vec<(String, f32, String)>,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_expression")?;
        let previous = self.commands.candidate_document().get_expression(&id);
        if previous.is_some_and(ExpressionAsset::has_extensions) {
            self.failed = true;
            return Err(edit_failure(
                py,
                "OPAQUE_EDIT_REQUIRES_IMPORT",
                "replace_expression",
                "Reimport the exp3 asset to edit one that contains unknown fields",
            ));
        }
        let file_type = previous.and_then(|asset| asset.file_type.clone());
        let mut parameters = Vec::with_capacity(entries.len());
        for (parameter_id, value, mode) in entries {
            let blend = match blend(&mode) {
                Ok(blend) => blend,
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            };
            parameters.push(ExpressionEntry {
                target: ExpressionTarget::Resolved { parameter_id },
                value,
                blend,
                extensions: Default::default(),
            });
        }
        self.commands
            .push(Command::ReplaceExpression(ExpressionAsset {
                id,
                name,
                file_type,
                fade_in,
                fade_out,
                entries: parameters,
                extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            }))
            .map_err(|error| sdk_failure(py, error))
    }

    fn import_expression3(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        text: &str,
    ) -> PyResult<Vec<(String, String, String)>> {
        self.ensure_open(py, "import_expression3")?;
        let result = self
            .commands
            .workspace
            .as_mut()
            .expect("edit is open")
            .import_expression3(id, name, text);
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
