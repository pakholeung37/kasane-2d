//! Typed edit commands and one-shot SDK publication.
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::conversion::*;
use crate::error::{edit_failure, poisoned, sdk_failure};
use kasane_core::{
    draw_order::DrawOrderGroup, Appearance, BindingAxis, Canvas, MeshBinding, MeshKeyform,
    Parameter, Part, RotationTransform, SceneBinding, SceneKeyform, Transform, TransformData, Vec2,
    WarpTransform,
};
use kasane_sdk::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
    AuthoringSession, EditReceipt, Version,
};
use pyo3::prelude::*;

enum Command {
    AddPng(kasane_core::ImageAsset),
    CreateRectangle(Box<kasane_core::Mesh>),
    RenameMesh(String, String),
    UpdatePositions(String, Vec<u32>, Vec<Vec2>),
    CreateParameter(Parameter),
    CreateMeshBinding(MeshBinding),
    ReplaceCanvas(Canvas),
    EraseObject(String),
    ReplaceParameter(Parameter),
    SetOrganizationParent(String, String),
    SetTransformParent(String, Option<String>),
    SetTransformPart(String, Option<String>),
    SetDeformParent(String, String),
    SetMeshPart(String, String),
    ReplaceDrawOrderGroups(Vec<DrawOrderGroup>),
    CreatePart(Part),
    ReplacePart(Part),
    CreateTransform(Transform),
    UpdateRotation(String, RotationTransform),
    UpdateWarpPoints(String, Vec<Vec2>),
    ReplaceAsset(kasane_core::ImageAsset),
    CreateSceneBinding(SceneBinding),
    ReplaceSceneBinding(SceneBinding),
    SetSceneKeyform(String, SceneKeyform),
}

#[pyclass]
pub(crate) struct NativeEdit {
    session: Arc<Mutex<AuthoringSession>>,
    label: String,
    expected: Version,
    commands: Vec<Command>,
    failed: bool,
    closed: bool,
}

impl NativeEdit {
    pub(crate) fn new(
        session: Arc<Mutex<AuthoringSession>>,
        label: String,
        expected: Version,
    ) -> Self {
        Self {
            session,
            label,
            expected,
            commands: Vec::new(),
            failed: false,
            closed: false,
        }
    }

    fn ensure_open(&self, py: Python<'_>, operation: &str) -> PyResult<()> {
        if self.closed {
            Err(edit_failure(
                py,
                "EDIT_CLOSED",
                operation,
                "Edit is already closed",
            ))
        } else if self.failed {
            Err(edit_failure(
                py,
                "EDIT_ABORTED",
                operation,
                "Edit was aborted by a previous error",
            ))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl NativeEdit {
    fn add_png_asset_from_base(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        base: &str,
        relative: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "add_png_asset_from_base")?;
        let (id, name, base, relative) = (
            id.to_owned(),
            name.to_owned(),
            base.to_owned(),
            relative.to_owned(),
        );
        match py.detach(move || {
            prepare_png_asset_from_base(&id, &name, Path::new(&base), Path::new(&relative))
        }) {
            Ok(asset) => {
                self.commands.push(Command::AddPng(asset));
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn replace_png_asset(
        &mut self,
        py: Python<'_>,
        id: &str,
        name: &str,
        path: &str,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_png_asset")?;
        let (id, name, path) = (id.to_owned(), name.to_owned(), path.to_owned());
        match py.detach(move || prepare_png_asset(&id, &name, Path::new(&path))) {
            Ok(asset) => {
                self.commands.push(Command::ReplaceAsset(asset));
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

    fn relocate_png_asset(&mut self, py: Python<'_>, id: &str, path: &str) -> PyResult<()> {
        self.ensure_open(py, "relocate_png_asset")?;
        let original = self.session.lock().map_err(|_| poisoned())?.asset(id);
        let Some(original) = original else {
            self.failed = true;
            return Err(edit_failure(
                py,
                "MISSING_ASSET",
                "relocate_png_asset",
                "Asset does not exist",
            ));
        };
        let path = path.to_owned();
        match py.detach(move || prepare_relocated_asset(&original, Path::new(&path))) {
            Ok(asset) => {
                self.commands.push(Command::ReplaceAsset(asset));
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
    }

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
        self.commands.push(Command::CreateTransform(Transform {
            id,
            name,
            part_id: part_id.map(Into::into),
            parent_id: parent_id.map(Into::into),
            data: TransformData::Rotation(rotation_data(rotation)),
            ..Transform::default()
        }));
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
        self.commands.push(Command::CreateTransform(Transform {
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
        }));
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
            .push(Command::UpdateRotation(id, rotation_data(rotation)));
        Ok(())
    }

    fn update_warp_points(
        &mut self,
        py: Python<'_>,
        id: String,
        points: Vec<PointTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "update_warp_points")?;
        self.commands.push(Command::UpdateWarpPoints(
            id,
            points.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
        ));
        Ok(())
    }

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
        self.commands.push(Command::CreatePart(Part {
            id,
            name,
            parent_id: parent_id.into(),
            enabled,
            draw_order,
            ..Part::default()
        }));
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
        let original = self.session.lock().map_err(|_| poisoned())?.part(&id);
        let mut part = original.unwrap_or_else(|| Part {
            runtime_id: id.clone(),
            ..Part::default()
        });
        part.id = id;
        part.name = name;
        part.parent_id = parent_id;
        part.enabled = enabled;
        part.draw_order = draw_order;
        self.commands.push(Command::ReplacePart(part));
        Ok(())
    }

    fn replace_draw_order_groups(
        &mut self,
        py: Python<'_>,
        groups: Vec<DrawOrderTuple>,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_draw_order_groups")?;
        self.commands.push(Command::ReplaceDrawOrderGroups(
            groups
                .into_iter()
                .map(|(owner, items, min_order, max_order)| DrawOrderGroup {
                    owner,
                    items,
                    min_order,
                    max_order,
                })
                .collect(),
        ));
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
        self.commands.push(Command::ReplaceCanvas(Canvas::new(
            width,
            height,
            Vec2::new(origin_x, origin_y),
            pixels_per_unit,
        )));
        Ok(())
    }

    fn erase_object(&mut self, py: Python<'_>, id: String) -> PyResult<()> {
        self.ensure_open(py, "erase_object")?;
        self.commands.push(Command::EraseObject(id));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (id, name, minimum, maximum, default_value, repeat=false))]
    fn replace_parameter(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        repeat: bool,
    ) -> PyResult<()> {
        self.ensure_open(py, "replace_parameter")?;
        let original = self.session.lock().map_err(|_| poisoned())?.parameter(&id);
        let mut parameter = original.unwrap_or_else(|| Parameter {
            runtime_id: id.clone(),
            ..Parameter::default()
        });
        parameter.id = id;
        parameter.name = name;
        parameter.minimum = minimum;
        parameter.maximum = maximum;
        parameter.default_value = default_value;
        parameter.repeat = repeat;
        self.commands.push(Command::ReplaceParameter(parameter));
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
            .push(Command::SetOrganizationParent(part_id, parent_id));
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
            .push(Command::SetTransformParent(transform_id, parent_id));
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
            .push(Command::SetTransformPart(transform_id, part_id));
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
            .push(Command::SetDeformParent(mesh_id, transform_id));
        Ok(())
    }

    fn set_mesh_part(&mut self, py: Python<'_>, mesh_id: String, part_id: String) -> PyResult<()> {
        self.ensure_open(py, "set_mesh_part")?;
        self.commands.push(Command::SetMeshPart(mesh_id, part_id));
        Ok(())
    }

    fn add_png_asset(&mut self, py: Python<'_>, id: &str, name: &str, path: &str) -> PyResult<()> {
        self.ensure_open(py, "add_png_asset")?;
        let id = id.to_owned();
        let name = name.to_owned();
        let path = path.to_owned();
        match py.detach(move || prepare_png_asset(&id, &name, Path::new(&path))) {
            Ok(asset) => {
                self.commands.push(Command::AddPng(asset));
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(sdk_failure(py, error))
            }
        }
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
                self.commands.push(Command::CreateRectangle(Box::new(mesh)));
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
        self.commands.push(Command::RenameMesh(id, name));
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
        self.commands.push(Command::UpdatePositions(
            id,
            vertex_ids,
            positions
                .into_iter()
                .map(|(x, y)| Vec2::new(x, y))
                .collect(),
        ));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (id, name, minimum, maximum, default_value, repeat=false))]
    fn create_parameter(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        minimum: f32,
        maximum: f32,
        default_value: f32,
        repeat: bool,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_parameter")?;
        self.commands.push(Command::CreateParameter(Parameter {
            id,
            name,
            minimum,
            maximum,
            default_value,
            repeat,
            ..Parameter::default()
        }));
        Ok(())
    }

    fn create_mesh_binding(
        &mut self,
        py: Python<'_>,
        id: String,
        mesh_id: String,
        axes: Vec<(String, Vec<f32>)>,
        forms: Vec<BindingForm>,
    ) -> PyResult<()> {
        self.ensure_open(py, "create_mesh_binding")?;
        self.commands.push(Command::CreateMeshBinding(MeshBinding {
            id,
            mesh_id,
            axes: axes
                .into_iter()
                .map(|(parameter_id, keys)| BindingAxis { parameter_id, keys })
                .collect(),
            keyforms: forms
                .into_iter()
                .map(|(keys, positions)| MeshKeyform {
                    keys,
                    positions: positions
                        .into_iter()
                        .map(|(x, y)| Vec2::new(x, y))
                        .collect(),
                    appearance: Appearance::default(),
                    draw_order: None,
                })
                .collect(),
        }));
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
            )?));
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
            )?));
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
        self.commands.push(Command::SetSceneKeyform(
            id,
            scene_form_from_tuple(kind, form)?,
        ));
        Ok(())
    }

    fn commit(&mut self, py: Python<'_>) -> PyResult<(u64, u64, u64)> {
        self.ensure_open(py, "commit")?;
        self.closed = true;
        let session = self.session.clone();
        let label = self.label.clone();
        let expected = self.expected;
        let commands = std::mem::take(&mut self.commands);
        let result = py.detach(move || {
            let mut session = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(session.edit(&label, Some(expected), |edit| {
                for command in commands {
                    match command {
                        Command::AddPng(asset) => edit.create_asset(asset)?,
                        Command::CreateRectangle(mesh) => edit.create_mesh(*mesh)?,
                        Command::RenameMesh(id, name) => edit.rename_mesh(&id, name)?,
                        Command::UpdatePositions(id, ids, positions) => {
                            edit.update_positions(&id, &ids, &positions)?
                        }
                        Command::CreateParameter(parameter) => edit.create_parameter(parameter)?,
                        Command::CreateMeshBinding(binding) => edit.create_binding(binding)?,
                        Command::ReplaceCanvas(canvas) => edit.replace_canvas(canvas)?,
                        Command::EraseObject(id) => edit.erase_object(&id)?,
                        Command::ReplaceParameter(parameter) => {
                            edit.replace_parameter(parameter)?
                        }
                        Command::SetOrganizationParent(id, parent) => {
                            edit.set_organization_parent(&id, &parent)?
                        }
                        Command::SetTransformParent(id, parent) => {
                            edit.set_transform_parent(&id, parent.map(Into::into))?
                        }
                        Command::SetTransformPart(id, part) => {
                            edit.set_transform_part(&id, part.map(Into::into))?
                        }
                        Command::SetDeformParent(id, parent) => {
                            edit.set_deform_parent(&id, &parent)?
                        }
                        Command::SetMeshPart(id, part) => edit.set_mesh_part(&id, &part)?,
                        Command::ReplaceDrawOrderGroups(groups) => {
                            edit.replace_draw_order_groups(groups)?
                        }
                        Command::CreatePart(part) => edit.create_part(part)?,
                        Command::ReplacePart(part) => edit.replace_part(part)?,
                        Command::CreateTransform(transform) => edit.create_transform(transform)?,
                        Command::UpdateRotation(id, rotation) => {
                            edit.update_rotation(&id, rotation)?
                        }
                        Command::UpdateWarpPoints(id, points) => {
                            edit.update_warp_points(&id, points)?
                        }
                        Command::ReplaceAsset(asset) => edit.replace_asset(asset)?,
                        Command::CreateSceneBinding(binding) => {
                            edit.create_scene_binding(binding)?
                        }
                        Command::ReplaceSceneBinding(binding) => {
                            edit.replace_scene_binding(binding)?
                        }
                        Command::SetSceneKeyform(id, form) => edit.set_scene_keyform(&id, form)?,
                    }
                }
                Ok(())
            }))
        });
        match result {
            Ok(Ok(((), EditReceipt { after, .. }))) => Ok(version_tuple(after)),
            Ok(Err(error)) => Err(sdk_failure(py, error)),
            Err(()) => Err(poisoned()),
        }
    }

    fn cancel(&mut self) {
        self.closed = true;
        self.commands.clear();
    }

    fn abort(&mut self) {
        self.failed = true;
        self.commands.clear();
    }
}
