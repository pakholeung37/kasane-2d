use super::*;

#[pymethods]
impl NativeSession {
    fn resolve_spatial_targets(
        &self,
        targets: Vec<(String, String)>,
    ) -> PyResult<Vec<Vec<String>>> {
        let snapshot = self.inner.lock().map_err(|_| poisoned())?.read_snapshot();
        let document = snapshot.document();
        let meshes: Vec<_> = document
            .mesh_order()
            .iter()
            .filter_map(|id| document.get_mesh(id).cloned())
            .collect();
        let parts: Vec<_> = document
            .part_order()
            .iter()
            .filter_map(|id| document.get_part(id).cloned())
            .collect();
        targets
            .into_iter()
            .map(|target| {
                let requested = crate::spatial::targets(vec![target])?;
                kasane_sdk::resolve_mesh_ids(&meshes, &parts, &requested)
                    .map_err(crate::spatial::spatial_error)
            })
            .collect()
    }
    fn mesh_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .mesh_ids()
            .to_vec())
    }

    fn asset_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .asset_ids()
            .to_vec())
    }

    fn parameter_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .parameter_ids()
            .to_vec())
    }

    fn binding_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .binding_ids()
            .to_vec())
    }

    fn part_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .part_ids()
            .to_vec())
    }

    fn transform_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .transform_ids()
            .to_vec())
    }

    fn scene_binding_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .scene_binding_ids()
            .to_vec())
    }

    fn blend_key_table_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .blend_key_table_ids()
            .to_vec())
    }

    fn blend_constraint_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .blend_constraint_ids()
            .to_vec())
    }

    fn blend_binding_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .blend_binding_ids()
            .to_vec())
    }

    fn glue_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .glue_ids()
            .to_vec())
    }

    fn offscreen_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .offscreen_ids()
            .to_vec())
    }
}
