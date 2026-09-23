use super::*;

#[pymethods]
impl NativeEdit {
    fn create_offscreen(&mut self, py: Python<'_>, data: OffscreenDataTuple) -> PyResult<()> {
        self.ensure_open(py, "create_offscreen")?;
        let runtime_id = data.0.clone();
        self.commands
            .push(Command::CreateOffscreen(offscreen_from_tuple(
                data, runtime_id,
            )))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_offscreen(&mut self, py: Python<'_>, data: OffscreenDataTuple) -> PyResult<()> {
        self.ensure_open(py, "replace_offscreen")?;
        let original = self.commands.candidate_document().get_offscreen(&data.0);
        let runtime_id = original
            .map(|value| value.runtime_id.clone())
            .unwrap_or_else(|| data.0.clone());
        self.commands
            .push(Command::ReplaceOffscreen(offscreen_from_tuple(
                data, runtime_id,
            )))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn create_glue(&mut self, py: Python<'_>, data: GlueDataTuple) -> PyResult<()> {
        self.ensure_open(py, "create_glue")?;
        let runtime_id = data.0.clone();
        self.commands
            .push(Command::CreateGlue(glue_from_tuple(data, runtime_id)))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_glue(&mut self, py: Python<'_>, data: GlueDataTuple) -> PyResult<()> {
        self.ensure_open(py, "replace_glue")?;
        let original = self.commands.candidate_document().get_glue(&data.0);
        let runtime_id = original
            .map(|value| value.runtime_id.clone())
            .unwrap_or_else(|| data.0.clone());
        self.commands
            .push(Command::ReplaceGlue(glue_from_tuple(data, runtime_id)))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn create_blend_key_table(
        &mut self,
        py: Python<'_>,
        id: String,
        parameter_id: String,
        keys: Vec<f32>,
        base_key_idx: usize,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_blend_key_table")?;
        self.commands
            .push(Command::CreateBlendKeyTable(BlendShapeKeyTable {
                id,
                parameter_id,
                keys,
                base_key_idx,
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_blend_key_table(
        &mut self,
        py: Python<'_>,
        id: String,
        parameter_id: String,
        keys: Vec<f32>,
        base_key_idx: usize,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_blend_key_table")?;
        self.commands
            .push(Command::ReplaceBlendKeyTable(BlendShapeKeyTable {
                id,
                parameter_id,
                keys,
                base_key_idx,
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn create_blend_constraint(
        &mut self,
        py: Python<'_>,
        id: String,
        parameter_id: String,
        keys: Vec<f32>,
        weights: Vec<f32>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_blend_constraint")?;
        self.commands
            .push(Command::CreateBlendConstraint(BlendShapeConstraint {
                id,
                parameter_id,
                keys,
                weights,
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_blend_constraint(
        &mut self,
        py: Python<'_>,
        id: String,
        parameter_id: String,
        keys: Vec<f32>,
        weights: Vec<f32>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_blend_constraint")?;
        self.commands
            .push(Command::ReplaceBlendConstraint(BlendShapeConstraint {
                id,
                parameter_id,
                keys,
                weights,
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn create_blend_binding(
        &mut self,
        py: Python<'_>,
        data: BlendBindingDataTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_blend_binding")?;
        self.commands
            .push(Command::CreateBlendBinding(blend_binding_from_tuple(data)?))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_blend_binding(
        &mut self,
        py: Python<'_>,
        data: BlendBindingDataTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_blend_binding")?;
        self.commands
            .push(Command::ReplaceBlendBinding(blend_binding_from_tuple(
                data,
            )?))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }
}
