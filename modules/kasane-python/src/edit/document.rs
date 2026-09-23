use super::*;

#[pymethods]
impl NativeEdit {
    #[pyo3(signature = (id, name, parent_id="", enabled=true, draw_order=0.0))]
    fn create_part(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        parent_id: &str,
        enabled: bool,
        draw_order: f32,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_part")?;
        self.commands
            .push(Command::CreatePart(Part {
                id,
                name,
                parent_id: parent_id.into(),
                enabled,
                draw_order,
                ..Part::default()
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_part(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        parent_id: String,
        enabled: bool,
        draw_order: f32,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_part")?;
        let original = self.commands.candidate_document().get_part(&id).cloned();
        let mut part = original.unwrap_or_else(|| Part {
            runtime_id: id.clone(),
            ..Part::default()
        });
        part.id = id;
        part.name = name;
        part.parent_id = parent_id;
        part.enabled = enabled;
        part.draw_order = draw_order;
        self.commands
            .push(Command::ReplacePart(part))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_draw_order_groups(
        &mut self,
        py: Python<'_>,
        groups: Vec<DrawOrderTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_draw_order_groups")?;
        self.commands
            .push(Command::ReplaceDrawOrderGroups(
                groups
                    .into_iter()
                    .map(|(owner, items, min_order, max_order)| DrawOrderGroup {
                        owner,
                        items,
                        min_order,
                        max_order,
                    })
                    .collect(),
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_canvas(
        &mut self,
        py: Python<'_>,
        width: f32,
        height: f32,
        origin_x: f32,
        origin_y: f32,
        pixels_per_unit: f32,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_canvas")?;
        self.commands
            .push(Command::ReplaceCanvas(Canvas::new(
                width,
                height,
                Vec2::new(origin_x, origin_y),
                pixels_per_unit,
            )))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn erase_object(&mut self, py: Python<'_>, id: String) -> PyResult<()> {
        self.ensure_open(py, "erase_object")?;
        self.commands
            .push(Command::EraseObject(id))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (id, name, minimum, maximum, default_value, repeat=false, kind=None))]
    fn replace_parameter(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        repeat: bool,
        kind: Option<&str>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_parameter")?;
        let mut parameter = Parameter {
            runtime_id: id.clone(),
            ..Parameter::default()
        };
        parameter.id = id;
        parameter.name = name;
        parameter.minimum = minimum;
        parameter.maximum = maximum;
        parameter.default_value = default_value;
        parameter.repeat = repeat;
        let kind = kind.map(parameter_kind_from_name).transpose()?;
        self.commands
            .push(Command::ReplaceParameter(parameter, kind))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_organization_parent(
        &mut self,
        py: Python<'_>,
        part_id: String,
        parent_id: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_organization_parent")?;
        self.commands
            .push(Command::SetOrganizationParent(part_id, parent_id))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_mesh_part(&mut self, py: Python<'_>, mesh_id: String, part_id: String) -> PyResult<()> {
        self.ensure_open(py, "set_mesh_part")?;
        self.commands
            .push(Command::SetMeshPart(mesh_id, part_id))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (id, name, minimum, maximum, default_value, repeat=false, kind="normal"))]
    fn create_parameter(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        repeat: bool,
        kind: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_parameter")?;
        self.commands
            .push(Command::CreateParameter(Parameter {
                id,
                name,
                minimum,
                maximum,
                default_value,
                repeat,
                kind: parameter_kind_from_name(kind)?,
                ..Parameter::default()
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }
}
