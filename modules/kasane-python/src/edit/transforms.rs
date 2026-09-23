use super::*;

#[pymethods]
impl NativeEdit {
    fn create_rotation_transform(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        part_id: Option<String>,
        parent_id: Option<String>,
        rotation: RotationTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_transform")?;
        self.commands
            .push(Command::CreateTransform(Transform {
                id,
                name,
                part_id: part_id.map(Into::into),
                parent_id: parent_id.map(Into::into),
                data: TransformData::Rotation(rotation_data(rotation)),
                ..Transform::default()
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_warp_transform(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        part_id: Option<String>,
        parent_id: Option<String>,
        rows: u32,
        columns: u32,
        quad: bool,
        points: Vec<PointTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_transform")?;
        self.commands
            .push(Command::CreateTransform(Transform {
                id,
                name,
                part_id: part_id.map(Into::into),
                parent_id: parent_id.map(Into::into),
                data: TransformData::Warp(WarpTransform {
                    rows,
                    columns,
                    quad,
                    points: points.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
                }),
                ..Transform::default()
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn update_rotation(
        &mut self,
        py: Python<'_>,
        id: String,
        rotation: RotationTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "update_rotation")?;
        self.commands
            .push(Command::UpdateRotation(id, rotation_data(rotation)))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_transform(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        part_id: Option<String>,
        parent_id: Option<String>,
        kind: &str,
        rotation: Option<RotationTuple>,
        warp: Option<WarpTuple>,
        enabled: bool,
        appearance: AppearanceTuple,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_transform")?;
        let data = match (kind, rotation, warp) {
            ("rotation", Some(rotation), None) => TransformData::Rotation(rotation_data(rotation)),
            ("warp", None, Some((rows, columns, quad, points))) => {
                TransformData::Warp(WarpTransform {
                    rows,
                    columns,
                    quad,
                    points: points.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
                })
            }
            _ => {
                return Err(PyValueError::new_err(
                    "Transform data does not match its kind",
                ))
            }
        };
        let original = self.commands.candidate_document().get_transform(&id);
        let runtime_id = original
            .map(|transform| transform.runtime_id.clone())
            .unwrap_or_else(|| id.clone());
        self.commands
            .push(Command::ReplaceTransform(Transform {
                id,
                runtime_id,
                name,
                part_id: part_id.map(Into::into),
                parent_id: parent_id.map(Into::into),
                data,
                enabled,
                appearance: appearance_from_tuple(appearance),
            }))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn update_warp_points(
        &mut self,
        py: Python<'_>,
        id: String,
        points: Vec<PointTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "update_warp_points")?;
        self.commands
            .push(Command::UpdateWarpPoints(
                id,
                points.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
            ))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_transform_parent(
        &mut self,
        py: Python<'_>,
        transform_id: String,
        parent_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_transform_parent")?;
        self.commands
            .push(Command::SetTransformParent(transform_id, parent_id))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_transform_part(
        &mut self,
        py: Python<'_>,
        transform_id: String,
        part_id: Option<String>,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_transform_part")?;
        self.commands
            .push(Command::SetTransformPart(transform_id, part_id))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }

    fn set_deform_parent(
        &mut self,
        py: Python<'_>,
        mesh_id: String,
        transform_id: String,
    ) -> PyResult<()> {
        self.ensure_open(py, "set_deform_parent")?;
        self.commands
            .push(Command::SetDeformParent(mesh_id, transform_id))
            .map_err(|error| sdk_failure(py, error))?;
        Ok(())
    }
}
