use super::*;

#[pymethods]
impl NativeEdit {
    fn create_mesh(&mut self, py: Python<'_>, data: MeshRecordDataTuple) -> PyResult<()> {
        self.ensure_open(py, "create_mesh")?;
        let runtime_id = data.0.clone();
        self.commands
            .push(Command::CreateMesh(Box::new(mesh_from_record(
                data, runtime_id,
            )?)))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_mesh(&mut self, py: Python<'_>, data: MeshRecordDataTuple) -> PyResult<()> {
        self.ensure_open(py, "replace_mesh")?;
        let original = self.commands.candidate_document().get_mesh(&data.0);
        let runtime_id = original
            .map(|value| value.runtime_id.clone())
            .unwrap_or_else(|| data.0.clone());
        self.commands
            .push(Command::ReplaceMesh(Box::new(mesh_from_record(
                data, runtime_id,
            )?)))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_topology(
        &mut self,
        py: Python<'_>,
        source: GeometryTuple,
        mesh_data: MeshRecordDataTuple,
        binding_data: Option<MeshBindingDataTuple>,
        blend_data: Vec<BlendBindingDataTuple>,
        glue_data: Vec<GlueDataTuple>,
        mapping: Vec<(u32, Option<u32>)>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_topology")?;
        let source = geometry_from_tuple(source)?;
        let candidate = self.commands.candidate_document();
        let runtime_id = candidate
            .get_mesh(&mesh_data.0)
            .map(|value| value.runtime_id.clone())
            .unwrap_or_else(|| mesh_data.0.clone());
        let mut glues = Vec::with_capacity(glue_data.len());
        for data in glue_data {
            let runtime_id = candidate
                .get_glue(&data.0)
                .map(|value| value.runtime_id.clone())
                .unwrap_or_else(|| data.0.clone());
            glues.push(glue_from_tuple(data, runtime_id));
        }
        let replacement = TopologyReplacement {
            mesh: mesh_from_record(mesh_data, runtime_id)?,
            binding: binding_data.map(mesh_binding_from_tuple),
            blend_bindings: blend_data
                .into_iter()
                .map(blend_binding_from_tuple)
                .collect::<PyResult<_>>()?,
            glues,
            vertex_mapping: mapping.into_iter().collect::<HashMap<_, _>>(),
        };
        self.commands
            .push(Command::ReplaceTopology(
                Box::new(source),
                Box::new(replacement),
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn create_rectangle(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        asset_id: &str,
        minimum: (f32, f32),
        maximum: (f32, f32),
    ) -> PyResult<()> {
        self.ensure_open(py, "create_rectangle")?;
        match rectangle_mesh(
            id,
            name,
            asset_id,
            Vec2::new(minimum.0, minimum.1),
            Vec2::new(maximum.0, maximum.1),
        ) {
            Ok(mesh) => {
                self.commands
                    .push(Command::CreateRectangle(Box::new(mesh)))
                    .map_err(|error| sdk_failure(py, error))?;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn rename_mesh(&mut self, py: Python<'_>, id: String, name: String) -> PyResult<()> {
        self.ensure_open(py, "rename_mesh")?;
        self.commands
            .push(Command::RenameMesh(id, name))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn update_positions(
        &mut self,
        py: Python<'_>,
        id: String,
        vertex_ids: Vec<u32>,
        positions: Vec<(f32, f32)>,
    ) -> PyResult<()> {
        self.ensure_open(py, "update_positions")?;
        self.commands
            .push(Command::UpdatePositions(
                id,
                vertex_ids,
                positions
                    .into_iter()
                    .map(|(x, y)| Vec2::new(x, y))
                    .collect(),
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn update_mesh_properties(
        &mut self,
        py: Python<'_>,
        id: String,
        texture_asset_id: String,
        appearance: AppearanceTuple,
        draw_order: Option<f32>,
        blend_mode: &str,
        enabled: bool,
        double_sided: bool,
        inverted_mask: bool,
        masks: Vec<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "update_mesh_properties")?;
        self.commands
            .push(Command::UpdateMeshProperties(
                id,
                MeshProperties {
                    texture_asset_id,
                    appearance: appearance_from_tuple(appearance),
                    draw_order,
                    blend_mode: blend_mode_from_name(blend_mode)?,
                    enabled,
                    double_sided,
                    inverted_mask,
                    masks,
                },
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }
}
