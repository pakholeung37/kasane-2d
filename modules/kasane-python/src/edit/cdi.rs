use super::*;
use kasane_core::document::{CdiCombinedSet, CdiParameterGroup, CdiParameterRef};

#[pymethods]
impl NativeEdit {
    fn set_parameter_display_name(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_parameter_display_name")?;
        self.commands
            .push(Command::SetParameterDisplayName(id, name))
            .map_err(|error| sdk_failure(py, error))
    }

    fn set_part_display_name(&mut self, py: Python<'_>, id: String, name: String) -> PyResult<()> {
        self.ensure_open(py, "set_part_display_name")?;
        self.commands
            .push(Command::SetPartDisplayName(id, name))
            .map_err(|error| sdk_failure(py, error))
    }

    #[pyo3(signature = (id, runtime_id, name, parent_id=None))]
    fn create_parameter_group(
        &mut self,
        py: Python<'_>,
        id: String,
        runtime_id: String,
        name: String,
        parent_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_parameter_group")?;
        self.commands
            .push(Command::CreateCdiGroup(CdiParameterGroup {
                id,
                runtime_id,
                name,
                parent_id,
                extensions: Default::default(),
            }))
            .map_err(|error| sdk_failure(py, error))
    }

    #[pyo3(signature = (id, runtime_id, name, parent_id=None))]
    fn replace_parameter_group(
        &mut self,
        py: Python<'_>,
        id: String,
        runtime_id: String,
        name: String,
        parent_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_parameter_group")?;
        let extensions = self
            .commands
            .candidate_document()
            .display_info()
            .parameter_groups
            .as_ref()
            .and_then(|groups| groups.iter().find(|group| group.id == id))
            .map(|group| group.extensions.clone())
            .unwrap_or_default();
        self.commands
            .push(Command::ReplaceCdiGroup(CdiParameterGroup {
                id,
                runtime_id,
                name,
                parent_id,
                extensions,
            }))
            .map_err(|error| sdk_failure(py, error))
    }

    fn set_parameter_group(
        &mut self,
        py: Python<'_>,
        parameter_id: String,
        group_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_parameter_group")?;
        self.commands
            .push(Command::SetParameterGroup(parameter_id, group_id))
            .map_err(|error| sdk_failure(py, error))
    }

    fn set_combined_parameters(
        &mut self,
        py: Python<'_>,
        id: String,
        parameter_ids: Vec<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_combined_parameters")?;
        self.commands
            .push(Command::SetCombinedParameters(CdiCombinedSet {
                id,
                members: parameter_ids
                    .into_iter()
                    .map(|parameter_id| CdiParameterRef::Resolved { parameter_id })
                    .collect(),
            }))
            .map_err(|error| sdk_failure(py, error))
    }

    fn import_cdi3(
        &mut self,
        py: Python<'_>,
        text: &str,
    ) -> PyResult<Vec<(String, String, String)>> {
        self.ensure_open(py, "import_cdi3")?;
        let result = self
            .commands
            .workspace
            .as_mut()
            .expect("edit is open")
            .import_cdi3(text);
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
