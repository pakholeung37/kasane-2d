use super::*;

#[pymethods]
impl NativeEdit {
    fn create_mesh_binding(
        &mut self,
        py: Python<'_>,
        id: String,
        mesh_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<BindingForm>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_mesh_binding")?;
        self.commands
            .push(Command::CreateMeshBinding(MeshBinding {
                id,
                mesh_id,
                axes: axes
                    .into_iter()
                    .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
                    .collect(),
                keyforms: forms
                    .into_iter()
                    .map(|(keys, positions, appearance, draw_order)| MeshKeyform {
                        keys,
                        positions: positions
                            .into_iter()
                            .map(|(x, y)| Vec2::new(x, y))
                            .collect(),
                        appearance: appearance_from_tuple(appearance),
                        draw_order,
                    })
                    .collect(),
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_mesh_binding(
        &mut self,
        py: Python<'_>,
        id: String,
        mesh_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<BindingForm>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_mesh_binding")?;
        self.commands
            .push(Command::ReplaceMeshBinding(MeshBinding {
                id,
                mesh_id,
                axes: axes
                    .into_iter()
                    .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
                    .collect(),
                keyforms: forms
                    .into_iter()
                    .map(|(keys, positions, appearance, draw_order)| MeshKeyform {
                        keys,
                        positions: positions
                            .into_iter()
                            .map(|(x, y)| Vec2::new(x, y))
                            .collect(),
                        appearance: appearance_from_tuple(appearance),
                        draw_order,
                    })
                    .collect(),
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_mesh_keyform(&mut self, py: Python<'_>, id: String, form: BindingForm) -> PyResult<()> {
        self.ensure_open(py, "set_mesh_keyform")?;
        let (keys, positions, appearance, draw_order) = form;
        self.commands
            .push(Command::SetMeshKeyform(
                id,
                MeshKeyform {
                    keys,
                    positions: positions
                        .into_iter()
                        .map(|(x, y)| Vec2::new(x, y))
                        .collect(),
                    appearance: appearance_from_tuple(appearance),
                    draw_order,
                },
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn create_scene_binding(
        &mut self,
        py: Python<'_>,
        id: String,
        kind: &str,
        target_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<SceneFormTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_scene_binding")?;
        self.commands
            .push(Command::CreateSceneBinding(scene_binding_from_tuples(
                id, kind, target_id, axes, forms,
            )?))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_scene_binding(
        &mut self,
        py: Python<'_>,
        id: String,
        kind: &str,
        target_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<SceneFormTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_scene_binding")?;
        self.commands
            .push(Command::ReplaceSceneBinding(scene_binding_from_tuples(
                id, kind, target_id, axes, forms,
            )?))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_scene_keyform(
        &mut self,
        py: Python<'_>,
        id: String,
        kind: &str,
        form: SceneFormTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_scene_keyform")?;
        self.commands
            .push(Command::SetSceneKeyform(
                id,
                scene_form_from_tuple(kind, form)?,
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn replace_part_binding_with_offscreen(
        &mut self,
        py: Python<'_>,
        binding_id: String,
        target_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<SceneFormTuple>,
        offscreen_data: OffscreenDataTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_part_binding_with_offscreen")?;
        let binding = scene_binding_from_tuples(binding_id, "part", target_id, axes, forms)?;
        let original = self
            .commands
            .candidate_document()
            .get_offscreen(&offscreen_data.0);
        let runtime_id = original
            .map(|value| value.runtime_id.clone())
            .unwrap_or_else(|| offscreen_data.0.clone());
        self.commands
            .push(Command::ReplacePartBindingWithOffscreen(
                binding,
                offscreen_from_tuple(offscreen_data, runtime_id),
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }
}
