//! Typed edit commands and one-shot SDK publication.
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::conversion::*;
use crate::error::{edit_failure, poisoned, sdk_failure};
use kasane_core::{
    draw_order::DrawOrderGroup, BindingAxis, BlendShapeConstraint, BlendShapeKeyTable, Canvas,
    MeshBinding, MeshKeyform, Parameter, ParameterKind, Part, RotationTransform, SceneBinding,
    SceneKeyform, Transform, TransformData, Vec2, WarpTransform,
};
use kasane_sdk::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
    AuthoringSession, EditSession, MeshProperties, SdkError, TopologyReplacement,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

enum Command {
    AddPng(kasane_core::ImageAsset),
    CreateRectangle(Box<kasane_core::Mesh>),
    CreateMesh(Box<kasane_core::Mesh>),
    ReplaceMesh(Box<kasane_core::Mesh>),
    ReplaceTopology(Box<kasane_sdk::GeometrySnapshot>, Box<TopologyReplacement>),
    RenameMesh(String, String),
    UpdatePositions(String, Vec<u32>, Vec<Vec2>),
    CreateParameter(Parameter),
    CreateMeshBinding(MeshBinding),
    ReplaceMeshBinding(MeshBinding),
    SetMeshKeyform(String, MeshKeyform),
    UpdateMeshProperties(String, MeshProperties),
    ReplaceCanvas(Canvas),
    EraseObject(String),
    ReplaceParameter(Parameter, Option<ParameterKind>),
    SetOrganizationParent(String, String),
    SetTransformParent(String, Option<String>),
    SetTransformPart(String, Option<String>),
    SetDeformParent(String, String),
    SetMeshPart(String, String),
    ReplaceDrawOrderGroups(Vec<DrawOrderGroup>),
    CreatePart(Part),
    ReplacePart(Part),
    CreateTransform(Transform),
    ReplaceTransform(Transform),
    CreateOffscreen(kasane_core::Offscreen),
    ReplaceOffscreen(kasane_core::Offscreen),
    ReplacePartBindingWithOffscreen(SceneBinding, kasane_core::Offscreen),
    CreateGlue(kasane_core::Glue),
    ReplaceGlue(kasane_core::Glue),
    CreateBlendKeyTable(BlendShapeKeyTable),
    ReplaceBlendKeyTable(BlendShapeKeyTable),
    CreateBlendConstraint(BlendShapeConstraint),
    ReplaceBlendConstraint(BlendShapeConstraint),
    CreateBlendBinding(kasane_core::BlendShapeBinding),
    ReplaceBlendBinding(kasane_core::BlendShapeBinding),
    UpdateRotation(String, RotationTransform),
    UpdateWarpPoints(String, Vec<Vec2>),
    ReplaceAsset(kasane_core::ImageAsset),
    CreateSceneBinding(SceneBinding),
    ReplaceSceneBinding(SceneBinding),
    SetSceneKeyform(String, SceneKeyform),
}

struct PendingCommands {
    workspace: Option<EditSession<'static>>,
    error: Option<SdkError>,
}

impl PendingCommands {
    fn new(workspace: EditSession<'static>) -> Self {
        Self {
            workspace: Some(workspace),
            error: None,
        }
    }

    fn push(&mut self, command: Command) -> Result<(), SdkError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        match command::apply_command(self.workspace.as_mut().expect("edit is open"), command) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.error = None;
    }

    fn candidate_document(&self) -> &kasane_core::Document {
        self.workspace
            .as_ref()
            .expect("edit is open")
            .candidate_document()
    }
}

#[pyclass]
pub(crate) struct NativeEdit {
    session: Arc<Mutex<AuthoringSession>>,
    active_edit: Arc<AtomicBool>,
    active_held: bool,
    commands: PendingCommands,
    failed: bool,
    closed: bool,
}

impl NativeEdit {
    pub(crate) fn new(
        session: Arc<Mutex<AuthoringSession>>,
        active_edit: Arc<AtomicBool>,
        workspace: EditSession<'static>,
    ) -> Self {
        Self {
            session,
            active_edit,
            active_held: true,
            commands: PendingCommands::new(workspace),
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
        } else if self.failed || self.commands.error.is_some() {
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

    fn release_active(&mut self) {
        if self.active_held {
            self.active_held = false;
            self.active_edit.store(false, Ordering::Release);
        }
    }
}

mod assets;
mod bindings;
mod command;
mod document;
mod effects;
mod lifecycle;
mod meshes;
mod transforms;

impl Drop for NativeEdit {
    fn drop(&mut self) {
        self.release_active();
    }
}
