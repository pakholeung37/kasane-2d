//! Authoring session lifecycle, project IO, reads, history and preview.
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::edit::EditSession;
use crate::types::*;
use kasane_animation::{ExpressionPreview, MotionPreview, PhysicsPreview};
use kasane_core::document::{
    DisplayInfo, ExpressionAsset, Model3Settings, MotionClip, MotionGroup, PackageAttachment,
    PhysicsAsset, PoseAsset, StructureIssue,
};
use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::preview::PreviewState;
use kasane_core::{
    evaluate_frame, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, Canvas,
    ChangeKind, Document, DrawableFrame, Glue, ImageAsset, Mesh, MeshBinding, Offscreen, Parameter,
    Part, PreviewValues, SceneBinding, Transform,
};
use kasane_project::{
    export_cdi3, export_expression3, export_motion3, export_physics3, export_pose3,
    CdiProjectError, ExpressionProjectError, MotionProjectError, PhysicsProjectError,
    PoseProjectError,
};
use kasane_project::{DocumentSession, FileSystem, NativeFileSystem};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

pub struct AuthoringSession {
    pub(crate) project: DocumentSession,
    pub(crate) preview: PreviewState,
    pub(crate) session_id: u64,
    pub(crate) generation: u64,
    pub(crate) done: VecDeque<HistoryEntry>,
    pub(crate) redo: Vec<HistoryEntry>,
    pub(crate) events: Vec<EditReceipt>,
    pub(crate) history_limits: HistoryLimits,
    pub(crate) incarnations: HashMap<ObjectKey, u64>,
    pub(crate) next_incarnation: u64,
}

/// Read-only document snapshot for bounded work outside an authoring lock.
/// The clone cost belongs to the capture step; later evaluation cannot observe edits.
#[derive(Debug, Clone)]
pub struct AuthoringSnapshot {
    document: Document,
    version: Version,
    project_path: Option<PathBuf>,
    clone_elapsed_ns: u64,
}

impl AuthoringSnapshot {
    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn project_path(&self) -> Option<&std::path::Path> {
        self.project_path.as_deref()
    }

    pub fn clone_elapsed_ns(&self) -> u64 {
        self.clone_elapsed_ns
    }

    pub fn evaluate(&self, values: &PreviewValues) -> Result<DrawableFrame, SdkError> {
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(&self.document, values, &mut frame);
        if status.is_ok() {
            Ok(frame)
        } else {
            Err(SdkError::from_status(status, "evaluate", Vec::new()))
        }
    }
}

impl AuthoringSession {
    /// Clone the committed document and source path under the caller's lock.
    /// History, events, and mutable preview state are not copied or changed.
    pub fn read_snapshot(&self) -> AuthoringSnapshot {
        let started = std::time::Instant::now();
        let document = self.project.document().clone();
        AuthoringSnapshot {
            document,
            version: self.version(),
            project_path: self.project_path().map(std::path::Path::to_path_buf),
            clone_elapsed_ns: started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        }
    }
    /// The caller supplies a canonical, nonzero, lowercase document UUID.
    pub fn new(document_id: &str, canvas: Canvas) -> Result<Self, SdkError> {
        Self::with_history_limits(document_id, canvas, HistoryLimits::default())
    }

    pub fn with_history_limits(
        document_id: &str,
        canvas: Canvas,
        history_limits: HistoryLimits,
    ) -> Result<Self, SdkError> {
        Self::with_options(
            document_id,
            canvas,
            history_limits,
            Arc::new(NativeFileSystem),
        )
    }

    /// Use a custom publication backend, primarily for deterministic IO tests.
    pub fn with_filesystem(
        document_id: &str,
        canvas: Canvas,
        filesystem: Arc<dyn FileSystem>,
    ) -> Result<Self, SdkError> {
        Self::with_options(document_id, canvas, HistoryLimits::default(), filesystem)
    }

    fn with_options(
        document_id: &str,
        canvas: Canvas,
        history_limits: HistoryLimits,
        filesystem: Arc<dyn FileSystem>,
    ) -> Result<Self, SdkError> {
        let mut document = Document::new();
        let status = document.initialize(document_id, canvas);
        if !status.is_ok() {
            return Err(SdkError::from_status(
                status,
                "new",
                vec![document_id.into()],
            ));
        }
        Ok(Self {
            project: DocumentSession::from_authoring_document_with_filesystem(document, filesystem),
            preview: PreviewState::default(),
            session_id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            generation: 1,
            done: VecDeque::new(),
            redo: Vec::new(),
            events: Vec::new(),
            history_limits,
            incarnations: HashMap::new(),
            next_incarnation: 1,
        })
    }

    pub fn version(&self) -> Version {
        Version {
            session_id: self.session_id,
            generation: self.generation,
            revision: self.project.document().revision(),
        }
    }
    pub fn evaluation_revision(&self) -> u64 {
        self.project.document().evaluation_revision()
    }
    pub fn canvas(&self) -> Canvas {
        self.project.document().canvas()
    }
    pub fn draw_order_groups(&self) -> Option<Vec<DrawOrderGroup>> {
        self.project
            .document()
            .draw_order_groups()
            .map(<[_]>::to_vec)
    }
    pub fn modified(&self) -> bool {
        self.project.document().modified()
    }
    pub fn asset_ids(&self) -> &[String] {
        self.project.document().asset_order()
    }
    pub fn mesh_ids(&self) -> &[String] {
        self.project.document().mesh_order()
    }
    pub fn parameter_ids(&self) -> &[String] {
        self.project.document().parameter_order()
    }
    pub fn binding_ids(&self) -> &[String] {
        self.project.document().binding_order()
    }
    pub fn part_ids(&self) -> &[String] {
        self.project.document().part_order()
    }
    pub fn transform_ids(&self) -> &[String] {
        self.project.document().transform_order()
    }
    pub fn scene_binding_ids(&self) -> &[String] {
        self.project.document().scene_binding_order()
    }
    pub fn blend_key_table_ids(&self) -> &[String] {
        self.project.document().blend_key_table_order()
    }
    pub fn blend_constraint_ids(&self) -> &[String] {
        self.project.document().blend_constraint_order()
    }
    pub fn blend_binding_ids(&self) -> &[String] {
        self.project.document().blend_binding_order()
    }
    pub fn glue_ids(&self) -> &[String] {
        self.project.document().glue_order()
    }
    pub fn offscreen_ids(&self) -> &[String] {
        self.project.document().offscreen_order()
    }
    pub fn display_info(&self) -> DisplayInfo {
        self.project.document().display_info().clone()
    }
    pub fn expression_ids(&self) -> &[String] {
        self.project.document().expression_order()
    }
    pub fn expression(&self, id: &str) -> Option<ExpressionAsset> {
        self.project.document().get_expression(id).cloned()
    }
    pub fn motion_ids(&self) -> &[String] {
        self.project.document().motion_order()
    }
    pub fn motion(&self, id: &str) -> Option<MotionClip> {
        self.project.document().get_motion(id).cloned()
    }
    pub fn motion_groups(&self) -> Vec<MotionGroup> {
        self.project.document().motion_groups().to_vec()
    }
    pub fn export_motion3(&self, id: &str) -> Result<String, SdkError> {
        export_motion3(self.project.document(), id).map_err(|error: MotionProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_motion3");
            result.field_path = Some(error.path.into());
            result.object_ids.push(id.into());
            result
        })
    }
    /// Capture an isolated expression runtime with deterministic seek/replay.
    pub fn expression_preview(&self) -> ExpressionPreview {
        ExpressionPreview::new(self.project.document())
    }
    pub fn motion_preview(&self) -> MotionPreview {
        let mut preview = MotionPreview::new(self.project.document());
        preview.bind_session_identity(self.session_id, self.generation);
        preview
    }
    pub fn physics_preview(&self) -> PhysicsPreview {
        PhysicsPreview::new(self.project.document())
    }
    pub fn pose(&self) -> Option<PoseAsset> {
        self.project.document().pose().cloned()
    }
    pub fn export_pose3(&self) -> Result<Option<String>, SdkError> {
        export_pose3(self.project.document()).map_err(|error: PoseProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_pose3");
            result.field_path = Some(error.path.into());
            result
        })
    }
    pub fn physics(&self) -> Option<PhysicsAsset> {
        self.project.document().physics().cloned()
    }
    pub fn missing_attachments(&self) -> Vec<String> {
        self.project.document().missing_attachments().to_vec()
    }
    pub fn model3_settings(&self) -> Model3Settings {
        self.project.document().model3_settings().clone()
    }
    pub fn package_attachments(&self) -> Vec<PackageAttachment> {
        self.project.document().package_attachments().to_vec()
    }
    pub fn export_physics3(&self) -> Result<Option<String>, SdkError> {
        export_physics3(self.project.document()).map_err(|error: PhysicsProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_physics3");
            result.field_path = Some(error.path.into());
            result
        })
    }
    pub fn export_expression3(&self, id: &str) -> Result<String, SdkError> {
        export_expression3(self.project.document(), id).map_err(|error: ExpressionProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_expression3");
            result.field_path = Some(error.path.into());
            result.object_ids.push(id.into());
            result
        })
    }
    pub fn export_cdi3(&self) -> Result<String, SdkError> {
        export_cdi3(self.project.document()).map_err(|error: CdiProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_cdi3");
            result.field_path = Some(error.path.into());
            result
        })
    }
    pub fn asset(&self, id: &str) -> Option<ImageAsset> {
        self.project.document().get_asset(id).cloned()
    }
    pub fn mesh(&self, id: &str) -> Option<Mesh> {
        self.project.document().get_mesh(id).cloned()
    }
    pub fn parameter(&self, id: &str) -> Option<Parameter> {
        self.project.document().get_parameter(id).cloned()
    }
    pub fn binding(&self, id: &str) -> Option<MeshBinding> {
        self.project.document().get_binding(id).cloned()
    }
    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<MeshBinding> {
        self.project.document().binding_for_mesh(mesh_id).cloned()
    }
    pub fn part(&self, id: &str) -> Option<Part> {
        self.project.document().get_part(id).cloned()
    }
    pub fn transform(&self, id: &str) -> Option<Transform> {
        self.project.document().get_transform(id).cloned()
    }
    pub fn scene_binding(&self, id: &str) -> Option<SceneBinding> {
        self.project.document().get_scene_binding(id).cloned()
    }
    pub fn binding_for_scene(&self, target_id: &str) -> Option<SceneBinding> {
        self.project
            .document()
            .binding_for_scene(target_id)
            .cloned()
    }
    pub fn references_to(&self, id: &str) -> Vec<String> {
        self.project.document().references_to(id)
    }
    pub fn validate_structure(&self) -> Vec<StructureIssue> {
        self.project.document().validate_structure()
    }
    pub fn blend_key_table(&self, id: &str) -> Option<BlendShapeKeyTable> {
        self.project.document().get_blend_key_table(id).cloned()
    }
    pub fn blend_constraint(&self, id: &str) -> Option<BlendShapeConstraint> {
        self.project.document().get_blend_constraint(id).cloned()
    }
    pub fn blend_binding(&self, id: &str) -> Option<BlendShapeBinding> {
        self.project.document().get_blend_binding(id).cloned()
    }
    pub fn glue(&self, id: &str) -> Option<Glue> {
        self.project.document().get_glue(id).cloned()
    }
    pub fn offscreen(&self, id: &str) -> Option<Offscreen> {
        self.project.document().get_offscreen(id).cloned()
    }
    pub fn handle(&self, kind: ObjectKind, id: &str) -> Result<ObjectHandle, SdkError> {
        let key = ObjectKey {
            kind,
            id: id.into(),
        };
        if !self.object_exists(kind, id) {
            let mut error = SdkError::new("NOT_FOUND", "Object does not exist", "handle");
            error.object_ids.push(id.into());
            return Err(error);
        }
        let incarnation = self
            .incarnations
            .get(&key)
            .copied()
            .expect("committed object has incarnation");
        Ok(ObjectHandle {
            session_id: self.session_id,
            generation: self.generation,
            object_id: id.into(),
            incarnation,
            kind,
        })
    }
    pub fn resolve_handle(&self, handle: &ObjectHandle) -> Result<(), SdkError> {
        let key = ObjectKey {
            kind: handle.kind,
            id: handle.object_id.clone(),
        };
        if handle.session_id != self.session_id
            || handle.generation != self.generation
            || self.incarnations.get(&key) != Some(&handle.incarnation)
            || !self.object_exists(handle.kind, handle.id())
        {
            let mut error = SdkError::new(
                "STALE_HANDLE",
                "Object handle is no longer valid",
                "resolve_handle",
            );
            error.object_ids.push(handle.object_id.clone());
            return Err(error);
        }
        Ok(())
    }
    pub fn mesh_by_handle(&self, handle: &ObjectHandle) -> Result<Mesh, SdkError> {
        self.resolve_handle(handle)?;
        if handle.kind != ObjectKind::Mesh {
            return Err(SdkError::new(
                "WRONG_OBJECT_KIND",
                "Handle is not a mesh",
                "mesh_by_handle",
            ));
        }
        Ok(self
            .mesh(handle.id())
            .expect("validated mesh handle resolves"))
    }
    pub fn find_meshes_by_name(&self, name: &str) -> Vec<Mesh> {
        self.mesh_ids()
            .iter()
            .filter_map(|id| self.project.document().get_mesh(id))
            .filter(|mesh| mesh.name == name)
            .cloned()
            .collect()
    }
    pub fn require_unique_mesh(&self, name: &str) -> Result<Mesh, SdkError> {
        let mut matches = self
            .mesh_ids()
            .iter()
            .filter_map(|id| self.project.document().get_mesh(id))
            .filter(|mesh| mesh.name == name);
        let Some(first) = matches.next() else {
            return Err(SdkError::new(
                "NOT_FOUND",
                "No mesh has this name",
                "require_unique_mesh",
            ));
        };
        if matches.next().is_some() {
            Err(SdkError::new(
                "AMBIGUOUS_NAME",
                "Several meshes have this name",
                "require_unique_mesh",
            ))
        } else {
            Ok(first.clone())
        }
    }
    pub fn geometry(&self, id: &str) -> Option<GeometrySnapshot> {
        let mesh = self.project.document().get_mesh(id)?;
        let space = if mesh.deformer_id.is_empty() {
            SourceSpace::CanvasPixels
        } else {
            SourceSpace::ParentLocal(mesh.deformer_id.clone())
        };
        Some(GeometrySnapshot {
            version: self.version(),
            mesh_id: id.into(),
            vertex_ids: mesh.vertex_ids.clone(),
            positions: mesh.base_positions.clone(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            space,
        })
    }
    pub fn history_lengths(&self) -> (usize, usize) {
        (self.done.len(), self.redo.len())
    }
    pub fn history_state(&self) -> HistoryState {
        HistoryState {
            undo_steps: self.done.len(),
            redo_steps: self.redo.len(),
            estimated_bytes: self
                .done
                .iter()
                .chain(self.redo.iter())
                .map(HistoryEntry::estimated_bytes)
                .sum(),
            max_steps: self.history_limits.max_steps,
            max_bytes: self.history_limits.max_bytes,
        }
    }
    pub fn estimated_content_bytes(&self) -> usize {
        self.project.document().estimated_content_bytes()
    }
    pub fn drain_events(&mut self) -> Vec<EditReceipt> {
        std::mem::take(&mut self.events)
    }

    pub fn begin_edit(
        &mut self,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> Result<EditSession<'_>, SdkError> {
        let mut edit = self.begin_owned_edit(label, expected)?;
        edit.session = Some(self);
        Ok(edit)
    }

    /// Create a persistent candidate workspace. `commit_to` checks the source
    /// version again before publishing, so callers can hold it across API calls.
    pub fn begin_owned_edit(
        &self,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> Result<EditSession<'static>, SdkError> {
        let version = self.version();
        if let Some(value) = expected.filter(|value| *value != version) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "begin_edit");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(version));
            return Err(error);
        }
        let candidate = self.project.document().fork_candidate();
        if candidate.estimated_content_bytes() > self.history_limits.max_bytes {
            return Err(SdkError::new(
                "HISTORY_LIMIT_EXCEEDED",
                "Candidate exceeds the history memory budget",
                "begin_edit",
            ));
        }
        Ok(EditSession {
            session: None,
            base_keys: self.object_keys(),
            candidate: Some(candidate),
            label: label.into(),
            before: version,
            aborted: false,
            kind: ChangeKind::None,
            object_ids: Vec::new(),
            erased_keys: HashSet::new(),
        })
    }

    pub fn edit<T>(
        &mut self,
        label: impl Into<String>,
        expected: Option<Version>,
        operation: impl FnOnce(&mut EditSession<'_>) -> Result<T, SdkError>,
    ) -> Result<(T, EditReceipt), SdkError> {
        let mut edit = self.begin_edit(label, expected)?;
        let value = operation(&mut edit)?;
        let receipt = edit.commit()?;
        Ok((value, receipt))
    }

    pub fn undo(&mut self) -> Result<EditReceipt, SdkError> {
        let before = self.version();
        let before_keys = self.object_keys();
        let entry = self
            .done
            .back_mut()
            .ok_or_else(|| SdkError::new("NOTHING_TO_UNDO", "History is empty", "undo"))?;
        self.project
            .swap_authoring_checkpoint(&mut entry.checkpoint)
            .map_err(|status| SdkError::from_status(status, "undo", Vec::new()))?;
        self.preview.retain_parameters(self.project.document());
        self.preview.invalidate();
        let entry = self.done.pop_back().expect("history entry still exists");
        self.refresh_incarnations(&before_keys, &entry.identity_keys);
        let receipt = EditReceipt {
            label: entry.label.clone(),
            before,
            after: self.version(),
            kind: ChangeKind::Structure,
            object_ids: Vec::new(),
            changed: true,
        };
        self.redo.push(entry);
        self.events.push(receipt.clone());
        Ok(receipt)
    }

    pub fn redo(&mut self) -> Result<EditReceipt, SdkError> {
        let before = self.version();
        let before_keys = self.object_keys();
        let entry = self
            .redo
            .last_mut()
            .ok_or_else(|| SdkError::new("NOTHING_TO_REDO", "History is empty", "redo"))?;
        self.project
            .swap_authoring_checkpoint(&mut entry.checkpoint)
            .map_err(|status| SdkError::from_status(status, "redo", Vec::new()))?;
        self.preview.retain_parameters(self.project.document());
        self.preview.invalidate();
        let entry = self.redo.pop().expect("history entry still exists");
        self.refresh_incarnations(&before_keys, &entry.identity_keys);
        let receipt = EditReceipt {
            label: entry.label.clone(),
            before,
            after: self.version(),
            kind: ChangeKind::Structure,
            object_ids: Vec::new(),
            changed: true,
        };
        self.done.push_back(entry);
        self.events.push(receipt.clone());
        Ok(receipt)
    }

    /// Evaluates explicit values without changing session preview state.
    pub fn evaluate(&self, values: &PreviewValues) -> Result<DrawableFrame, SdkError> {
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(self.project.document(), values, &mut frame);
        if status.is_ok() {
            Ok(frame)
        } else {
            Err(SdkError::from_status(status, "evaluate", Vec::new()))
        }
    }

    pub fn preview_values(&self) -> &PreviewValues {
        self.preview.values()
    }
    pub fn preview_revision(&self) -> u64 {
        self.preview.revision()
    }
    pub fn preview_evaluation_count(&self) -> u64 {
        self.preview.evaluation_count()
    }
    pub fn preview_frame(&mut self) -> Result<Arc<DrawableFrame>, SdkError> {
        self.preview
            .frame(self.project.document(), self.generation)
            .map_err(|status| SdkError::from_status(status, "preview_frame", Vec::new()))
    }
    pub fn set_preview_values(&mut self, values: PreviewValues) -> Result<bool, SdkError> {
        self.preview
            .replace(self.project.document(), self.generation, values)
            .map_err(|status| SdkError::from_status(status, "set_preview_values", Vec::new()))
    }
    pub fn set_preview_parameter(&mut self, id: &str, value: f32) -> Result<bool, SdkError> {
        let mut values = self.preview.values().clone();
        values.insert(id.into(), value);
        self.preview
            .replace(self.project.document(), self.generation, values)
            .map_err(|status| {
                SdkError::from_status(status, "set_preview_parameter", vec![id.into()])
            })
    }
    pub fn reset_preview_values(&mut self) -> Result<bool, SdkError> {
        self.set_preview_values(PreviewValues::new())
    }

    pub(crate) fn object_keys(&self) -> HashSet<ObjectKey> {
        let doc = self.project.document();
        let mut keys: HashSet<_> = [
            (ObjectKind::Asset, doc.asset_order()),
            (ObjectKind::Mesh, doc.mesh_order()),
            (ObjectKind::Parameter, doc.parameter_order()),
            (ObjectKind::MeshBinding, doc.binding_order()),
            (ObjectKind::Part, doc.part_order()),
            (ObjectKind::Transform, doc.transform_order()),
            (ObjectKind::SceneBinding, doc.scene_binding_order()),
            (ObjectKind::BlendKeyTable, doc.blend_key_table_order()),
            (ObjectKind::BlendConstraint, doc.blend_constraint_order()),
            (ObjectKind::BlendBinding, doc.blend_binding_order()),
            (ObjectKind::Glue, doc.glue_order()),
            (ObjectKind::Offscreen, doc.offscreen_order()),
        ]
        .into_iter()
        .flat_map(|(kind, ids)| {
            ids.iter().map(move |id| ObjectKey {
                kind,
                id: id.clone(),
            })
        })
        .collect();
        if let Some(groups) = &doc.display_info().parameter_groups {
            keys.extend(groups.iter().map(|group| ObjectKey {
                kind: ObjectKind::CdiParameterGroup,
                id: group.id.clone(),
            }));
        }
        if let Some(sets) = &doc.display_info().combined_parameters {
            keys.extend(sets.iter().map(|set| ObjectKey {
                kind: ObjectKind::CdiCombinedSet,
                id: set.id.clone(),
            }));
        }
        keys.extend(doc.expression_order().iter().map(|id| ObjectKey {
            kind: ObjectKind::Expression,
            id: id.clone(),
        }));
        keys.extend(doc.motion_order().iter().map(|id| ObjectKey {
            kind: ObjectKind::Motion,
            id: id.clone(),
        }));
        if let Some(pose) = doc.pose() {
            keys.insert(ObjectKey {
                kind: ObjectKind::Pose,
                id: pose.id.clone(),
            });
        }
        if let Some(physics) = doc.physics() {
            keys.insert(ObjectKey {
                kind: ObjectKind::Physics,
                id: physics.id.clone(),
            });
        }
        keys
    }

    fn object_exists(&self, kind: ObjectKind, id: &str) -> bool {
        object_exists_in(self.project.document(), kind, id)
    }

    pub(crate) fn refresh_incarnations(
        &mut self,
        before: &HashSet<ObjectKey>,
        identity_keys: &HashSet<ObjectKey>,
    ) {
        let after = self.object_keys();
        let mut changed: HashSet<_> = before.symmetric_difference(&after).cloned().collect();
        changed.extend(
            identity_keys
                .iter()
                .filter(|key| after.contains(*key))
                .cloned(),
        );
        for key in changed {
            self.incarnations.insert(key, self.next_incarnation);
            self.next_incarnation = self
                .next_incarnation
                .checked_add(1)
                .expect("incarnation counter exhausted");
        }
    }
}

pub(crate) fn object_exists_in(doc: &Document, kind: ObjectKind, id: &str) -> bool {
    match kind {
        ObjectKind::Asset => doc.get_asset(id).is_some(),
        ObjectKind::Mesh => doc.get_mesh(id).is_some(),
        ObjectKind::Parameter => doc.get_parameter(id).is_some(),
        ObjectKind::MeshBinding => doc.get_binding(id).is_some(),
        ObjectKind::Part => doc.get_part(id).is_some(),
        ObjectKind::Transform => doc.get_transform(id).is_some(),
        ObjectKind::SceneBinding => doc.get_scene_binding(id).is_some(),
        ObjectKind::BlendKeyTable => doc.get_blend_key_table(id).is_some(),
        ObjectKind::BlendConstraint => doc.get_blend_constraint(id).is_some(),
        ObjectKind::BlendBinding => doc.get_blend_binding(id).is_some(),
        ObjectKind::Glue => doc.get_glue(id).is_some(),
        ObjectKind::Offscreen => doc.get_offscreen(id).is_some(),
        ObjectKind::CdiParameterGroup => doc
            .display_info()
            .parameter_groups
            .as_ref()
            .is_some_and(|groups| groups.iter().any(|group| group.id == id)),
        ObjectKind::CdiCombinedSet => doc
            .display_info()
            .combined_parameters
            .as_ref()
            .is_some_and(|sets| sets.iter().any(|set| set.id == id)),
        ObjectKind::Expression => doc.get_expression(id).is_some(),
        ObjectKind::Motion => doc.get_motion(id).is_some(),
        ObjectKind::Pose => doc.pose().is_some_and(|pose| pose.id == id),
        ObjectKind::Physics => doc.physics().is_some_and(|physics| physics.id == id),
    }
}
