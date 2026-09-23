use super::*;

#[pymethods]
impl NativeSession {
    fn asset(&self, id: &str) -> PyResult<Option<AssetTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.asset(id).map(|asset| {
            (
                asset.id,
                asset.name,
                asset.source,
                asset.width,
                asset.height,
                asset.sha256,
                version_tuple(session.version()),
            )
        }))
    }

    fn references_to(&self, id: &str) -> PyResult<Vec<String>> {
        Ok(self.inner.lock().map_err(|_| poisoned())?.references_to(id))
    }

    fn parameter(&self, id: &str) -> PyResult<Option<ParameterTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.parameter(id).map(|parameter| {
            (
                parameter.id,
                parameter.name,
                parameter.minimum,
                parameter.maximum,
                parameter.default_value,
                parameter.repeat,
                parameter_kind_name(parameter.kind).to_owned(),
                version_tuple(session.version()),
            )
        }))
    }

    fn mesh(&self, id: &str) -> PyResult<Option<MeshTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .mesh(id)
            .map(|mesh| mesh_tuple(mesh, session.version())))
    }

    fn mesh_record(&self, id: &str) -> PyResult<Option<MeshRecordTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .mesh(id)
            .map(|value| mesh_record_tuple(value, session.version())))
    }

    fn mesh_properties(&self, id: &str) -> PyResult<Option<MeshPropertiesTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.mesh(id).map(|mesh| {
            let props = MeshProperties::from(&mesh);
            (
                props.texture_asset_id,
                appearance_tuple(props.appearance),
                props.draw_order,
                blend_mode_name(props.blend_mode).into(),
                props.enabled,
                props.double_sided,
                props.inverted_mask,
                props.masks,
                version_tuple(session.version()),
            )
        }))
    }

    fn binding(&self, id: &str) -> PyResult<Option<MeshBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .binding(id)
            .map(|binding| binding_tuple(binding, session.version())))
    }

    fn binding_for_mesh(&self, mesh_id: &str) -> PyResult<Option<MeshBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .binding_for_mesh(mesh_id)
            .map(|binding| binding_tuple(binding, session.version())))
    }

    fn part(&self, id: &str) -> PyResult<Option<PartTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.part(id).map(|part| {
            (
                part.id,
                part.runtime_id,
                part.name,
                part.parent_id,
                part.enabled,
                part.draw_order,
                version_tuple(session.version()),
            )
        }))
    }

    fn transform(&self, id: &str) -> PyResult<Option<TransformTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .transform(id)
            .map(|transform| transform_tuple(transform, session.version())))
    }

    fn offscreen(&self, id: &str) -> PyResult<Option<OffscreenTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .offscreen(id)
            .map(|value| offscreen_tuple(value, session.version())))
    }

    fn glue(&self, id: &str) -> PyResult<Option<GlueTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .glue(id)
            .map(|value| glue_tuple(value, session.version())))
    }

    fn blend_key_table(&self, id: &str) -> PyResult<Option<BlendKeyTableTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.blend_key_table(id).map(|value| {
            (
                value.id,
                value.parameter_id,
                value.keys,
                value.base_key_idx,
                version_tuple(session.version()),
            )
        }))
    }

    fn blend_constraint(&self, id: &str) -> PyResult<Option<BlendConstraintTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session.blend_constraint(id).map(|value| {
            (
                value.id,
                value.parameter_id,
                value.keys,
                value.weights,
                version_tuple(session.version()),
            )
        }))
    }

    fn blend_binding(&self, id: &str) -> PyResult<Option<BlendBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .blend_binding(id)
            .map(|value| blend_binding_tuple(value, session.version())))
    }

    fn scene_binding(&self, id: &str) -> PyResult<Option<SceneBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .scene_binding(id)
            .map(|binding| scene_binding_tuple(binding, session.version())))
    }

    fn binding_for_scene(&self, target_id: &str) -> PyResult<Option<SceneBindingTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .binding_for_scene(target_id)
            .map(|binding| scene_binding_tuple(binding, session.version())))
    }

    fn handle(&self, py: Python<'_>, kind: &str, id: &str) -> PyResult<NativeHandle> {
        let kind = object_kind(kind)?;
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .handle(kind, id)
            .map(|inner| NativeHandle { inner })
            .map_err(|error| sdk_failure(py, error))
    }

    fn resolve_handle(&self, py: Python<'_>, handle: PyRef<'_, NativeHandle>) -> PyResult<()> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .resolve_handle(&handle.inner)
            .map_err(|error| sdk_failure(py, error))
    }

    fn mesh_by_handle(
        &self,
        py: Python<'_>,
        handle: PyRef<'_, NativeHandle>,
    ) -> PyResult<MeshTuple> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .mesh_by_handle(&handle.inner)
            .map(|mesh| mesh_tuple(mesh, session.version()))
            .map_err(|error| sdk_failure(py, error))
    }

    fn find_meshes_by_name(&self, name: &str) -> PyResult<Vec<MeshTuple>> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(session
            .find_meshes_by_name(name)
            .into_iter()
            .map(|mesh| mesh_tuple(mesh, session.version()))
            .collect())
    }

    fn require_unique_mesh(&self, py: Python<'_>, name: &str) -> PyResult<MeshTuple> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .require_unique_mesh(name)
            .map(|mesh| mesh_tuple(mesh, session.version()))
            .map_err(|error| sdk_failure(py, error))
    }

    fn geometry(&self, id: &str) -> PyResult<Option<GeometryTuple>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .geometry(id)
            .map(|geometry| {
                let (space, parent) = match geometry.space {
                    SourceSpace::CanvasPixels => ("canvas_pixels".into(), None),
                    SourceSpace::ParentLocal(parent) => ("parent_local".into(), Some(parent)),
                };
                (
                    version_tuple(geometry.version),
                    geometry.mesh_id,
                    geometry.vertex_ids,
                    geometry.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                    geometry.uvs.into_iter().map(|p| (p.x, p.y)).collect(),
                    geometry
                        .triangles
                        .into_iter()
                        .map(|t| (t[0], t[1], t[2]))
                        .collect(),
                    space,
                    parent,
                )
            }))
    }
}
