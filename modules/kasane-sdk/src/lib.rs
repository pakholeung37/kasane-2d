//! In-memory Rust authoring SDK. Project IO and external harnesses are later stages.
mod diagnostics;

pub use diagnostics::{GeometryBounds, GeometryChecks, GeometryDiagnostic, GeometryDiagnosticKind};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use kasane_core::document::{DocumentCheckpoint, StructureIssue};
use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::preview::PreviewState;
use kasane_core::{
    evaluate_frame, Appearance, BlendMode, BlendShapeBinding, BlendShapeConstraint,
    BlendShapeKeyTable, Canvas, ChangeKind, Document, DrawableFrame, EditResult, Glue, ImageAsset,
    Mesh, MeshBinding, MeshKeyform, Offscreen, Parameter, Part, PartId, PreviewValues,
    RotationTransform, SceneBinding, SceneKeyform, Status, Transform, TransformData, TransformId,
    Vec2, VertexId,
};
use kasane_project::{decode_png, DocumentSession};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub session_id: u64,
    pub generation: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkError {
    pub code: Box<str>,
    pub message: Box<str>,
    pub operation: &'static str,
    pub object_ids: Vec<String>,
    pub field_path: Option<Box<str>>,
    pub expected_version: Option<Box<Version>>,
    pub actual_version: Option<Box<Version>>,
    pub referrers: Box<[String]>,
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SdkError {}

impl SdkError {
    fn new(code: &str, message: &str, operation: &'static str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            operation,
            object_ids: Vec::new(),
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        }
    }
    fn from_status(status: Status, operation: &'static str, ids: Vec<String>) -> Self {
        Self {
            code: status.code.into_boxed_str(),
            message: status.message.into_boxed_str(),
            operation,
            object_ids: ids,
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditReceipt {
    pub label: String,
    pub before: Version,
    pub after: Version,
    pub kind: ChangeKind,
    pub object_ids: Vec<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSpace {
    CanvasPixels,
    ParentLocal(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometrySnapshot {
    pub version: Version,
    pub mesh_id: String,
    pub vertex_ids: Vec<VertexId>,
    pub positions: Vec<Vec2>,
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<[VertexId; 3]>,
    pub space: SourceSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Asset,
    Mesh,
    Parameter,
    MeshBinding,
    Part,
    Transform,
    SceneBinding,
    BlendKeyTable,
    BlendConstraint,
    BlendBinding,
    Glue,
    Offscreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHandle {
    session_id: u64,
    generation: u64,
    object_id: String,
    incarnation: u64,
    kind: ObjectKind,
}

impl ObjectHandle {
    pub fn id(&self) -> &str {
        &self.object_id
    }
    pub fn kind(&self) -> ObjectKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ObjectKey {
    kind: ObjectKind,
    id: String,
}

/// Editable display and drawing fields. Geometry, persistent ID and runtime ID
/// are preserved by `update_mesh_properties`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshProperties {
    pub texture_asset_id: String,
    pub appearance: Appearance,
    pub draw_order: Option<f32>,
    pub blend_mode: BlendMode,
    pub enabled: bool,
    pub double_sided: bool,
    pub inverted_mask: bool,
    pub masks: Vec<String>,
}

impl From<&Mesh> for MeshProperties {
    fn from(mesh: &Mesh) -> Self {
        Self {
            texture_asset_id: mesh.texture_asset_id.clone(),
            appearance: mesh.appearance,
            draw_order: mesh.draw_order,
            blend_mode: mesh.blend_mode,
            enabled: mesh.enabled,
            double_sided: mesh.double_sided,
            inverted_mask: mesh.inverted_mask,
            masks: mesh.masks.clone(),
        }
    }
}

/// All geometry dependencies are explicit. For an unbound mesh, pass None and
/// empty vectors; mapping must mention every previous vertex ID exactly once.
#[derive(Debug, Clone)]
pub struct TopologyReplacement {
    pub mesh: Mesh,
    pub binding: Option<MeshBinding>,
    pub blend_bindings: Vec<BlendShapeBinding>,
    pub glues: Vec<Glue>,
    pub vertex_mapping: HashMap<VertexId, Option<VertexId>>,
}

struct HistoryEntry {
    label: String,
    checkpoint: DocumentCheckpoint,
    identity_keys: HashSet<ObjectKey>,
}

impl HistoryEntry {
    fn estimated_bytes(&self) -> usize {
        self.checkpoint.estimated_bytes()
            + self.label.capacity()
            + std::mem::size_of::<String>()
            + self.identity_keys.capacity() * std::mem::size_of::<ObjectKey>()
            + self
                .identity_keys
                .iter()
                .map(|key| key.id.capacity())
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_steps: usize,
    pub max_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_steps: 50,
            max_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryState {
    pub undo_steps: usize,
    pub redo_steps: usize,
    pub estimated_bytes: usize,
    pub max_steps: usize,
    pub max_bytes: usize,
}

pub struct AuthoringSession {
    project: DocumentSession,
    preview: PreviewState,
    session_id: u64,
    generation: u64,
    done: VecDeque<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    events: Vec<EditReceipt>,
    history_limits: HistoryLimits,
    incarnations: HashMap<ObjectKey, u64>,
    next_incarnation: u64,
}

impl AuthoringSession {
    /// The caller supplies a canonical, nonzero, lowercase document UUID.
    pub fn new(document_id: &str, canvas: Canvas) -> Result<Self, SdkError> {
        Self::with_history_limits(document_id, canvas, HistoryLimits::default())
    }

    pub fn with_history_limits(
        document_id: &str,
        canvas: Canvas,
        history_limits: HistoryLimits,
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
            project: DocumentSession::from_authoring_document(document),
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

    /// Atomically replace the in-memory document. A failed request leaves the
    /// current document, history, preview and handles intact.
    pub fn new_project(
        &mut self,
        document_id: &str,
        canvas: Canvas,
        expected: Option<Version>,
    ) -> Result<Version, SdkError> {
        let current = self.version();
        if let Some(value) = expected.filter(|value| *value != current) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "new_project");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(current));
            return Err(error);
        }
        let mut document = Document::new();
        let status = document.initialize(document_id, canvas);
        if !status.is_ok() {
            return Err(SdkError::from_status(
                status,
                "new_project",
                vec![document_id.into()],
            ));
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            SdkError::new(
                "GENERATION_EXHAUSTED",
                "Document generation exhausted",
                "new_project",
            )
        })?;
        self.project = DocumentSession::from_authoring_document(document);
        self.generation = generation;
        self.done.clear();
        self.redo.clear();
        self.preview.reset();
        self.incarnations.clear();
        self.events.clear();
        Ok(self.version())
    }

    pub fn document_id(&self) -> &str {
        self.project.document().id()
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
            .filter_map(|id| self.mesh(id))
            .filter(|mesh| mesh.name == name)
            .collect()
    }
    pub fn require_unique_mesh(&self, name: &str) -> Result<Mesh, SdkError> {
        let mut matches = self.find_meshes_by_name(name);
        match matches.len() {
            0 => Err(SdkError::new(
                "NOT_FOUND",
                "No mesh has this name",
                "require_unique_mesh",
            )),
            1 => Ok(matches.pop().unwrap()),
            _ => Err(SdkError::new(
                "AMBIGUOUS_NAME",
                "Several meshes have this name",
                "require_unique_mesh",
            )),
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
            session: self,
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

    fn object_keys(&self) -> HashSet<ObjectKey> {
        let doc = self.project.document();
        [
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
        .collect()
    }

    fn object_exists(&self, kind: ObjectKind, id: &str) -> bool {
        object_exists_in(self.project.document(), kind, id)
    }

    fn refresh_incarnations(
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

fn object_exists_in(doc: &Document, kind: ObjectKind, id: &str) -> bool {
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
    }
}

pub struct EditSession<'a> {
    session: &'a mut AuthoringSession,
    candidate: Option<Document>,
    label: String,
    before: Version,
    aborted: bool,
    kind: ChangeKind,
    object_ids: Vec<String>,
    erased_keys: HashSet<ObjectKey>,
}

impl EditSession<'_> {
    fn document(&mut self) -> &mut Document {
        self.candidate.as_mut().expect("edit candidate exists")
    }
    fn record(
        &mut self,
        result: EditResult,
        operation: &'static str,
        id: &str,
    ) -> Result<(), SdkError> {
        if !result.status.is_ok() {
            self.aborted = true;
            let mut error = SdkError::from_status(result.status, operation, vec![id.into()]);
            error.referrers = result.referrers.into_boxed_slice();
            return Err(error);
        }
        self.kind = merge_kind(self.kind, result.changes.kind);
        for id in result.changes.object_ids {
            if !self.object_ids.contains(&id) {
                self.object_ids.push(id);
            }
        }
        Ok(())
    }
    fn ensure_active(&self, operation: &'static str) -> Result<(), SdkError> {
        if self.aborted {
            Err(SdkError::new(
                "EDIT_ABORTED",
                "An earlier edit method failed",
                operation,
            ))
        } else {
            Ok(())
        }
    }
    fn abort_with(&mut self, error: SdkError) -> SdkError {
        self.aborted = true;
        error
    }
    pub fn replace_canvas(&mut self, canvas: Canvas) -> Result<(), SdkError> {
        self.ensure_active("replace_canvas")?;
        let id = self.document().id().to_owned();
        let result = self.document().replace_canvas(canvas);
        self.record(result, "replace_canvas", &id)
    }
    pub fn replace_draw_order_groups(
        &mut self,
        groups: Vec<DrawOrderGroup>,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_draw_order_groups")?;
        let id = self.document().id().to_owned();
        let result = self.document().replace_draw_order_groups(groups);
        self.record(result, "replace_draw_order_groups", &id)
    }
    pub fn erase_object(&mut self, id: &str) -> Result<(), SdkError> {
        self.ensure_active("erase_object")?;
        let original = self
            .session
            .object_keys()
            .into_iter()
            .find(|key| key.id == id);
        let result = self.document().erase_object(id);
        self.record(result, "erase_object", id)?;
        if let Some(key) = original {
            self.erased_keys.insert(key);
        }
        Ok(())
    }
    pub fn create_asset(&mut self, asset: ImageAsset) -> Result<(), SdkError> {
        self.ensure_active("create_asset")?;
        let id = asset.id.clone();
        let result = self.document().add_asset(asset);
        self.record(result, "create_asset", &id)
    }
    pub fn replace_asset(&mut self, asset: ImageAsset) -> Result<(), SdkError> {
        self.ensure_active("replace_asset")?;
        let id = asset.id.clone();
        let result = self.document().replace_asset(asset);
        self.record(result, "replace_asset", &id)
    }
    pub fn create_parameter(&mut self, parameter: Parameter) -> Result<(), SdkError> {
        self.ensure_active("create_parameter")?;
        let id = parameter.id.clone();
        let result = self.document().create_parameter(parameter);
        self.record(result, "create_parameter", &id)
    }
    pub fn replace_parameter(&mut self, parameter: Parameter) -> Result<(), SdkError> {
        self.ensure_active("replace_parameter")?;
        let id = parameter.id.clone();
        let result = self.document().replace_parameter(parameter);
        self.record(result, "replace_parameter", &id)
    }
    pub fn create_part(&mut self, part: Part) -> Result<(), SdkError> {
        self.ensure_active("create_part")?;
        let id = part.id.clone();
        let result = self.document().create_part(part);
        self.record(result, "create_part", &id)
    }
    pub fn replace_part(&mut self, part: Part) -> Result<(), SdkError> {
        self.ensure_active("replace_part")?;
        let id = part.id.clone();
        let result = self.document().replace_part(part);
        self.record(result, "replace_part", &id)
    }
    /// Organization hierarchy is independent of the transform hierarchy.
    pub fn set_organization_parent(
        &mut self,
        part_id: &str,
        parent_id: &str,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_organization_parent")?;
        let Some(mut part) = self.document().get_part(part_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_PART", "Part does not exist"),
                "set_organization_parent",
                vec![part_id.into()],
            )));
        };
        part.parent_id = parent_id.into();
        let result = self.document().replace_part(part);
        self.record(result, "set_organization_parent", part_id)
    }
    pub fn create_transform(&mut self, transform: Transform) -> Result<(), SdkError> {
        self.ensure_active("create_transform")?;
        let id = transform.id.clone();
        let result = self.document().create_transform(transform);
        self.record(result, "create_transform", &id)
    }
    pub fn replace_transform(&mut self, transform: Transform) -> Result<(), SdkError> {
        self.ensure_active("replace_transform")?;
        let id = transform.id.clone();
        let result = self.document().replace_transform(transform);
        self.record(result, "replace_transform", &id)
    }
    /// Changes the transform parent while retaining the stored local coordinates.
    pub fn set_transform_parent(
        &mut self,
        transform_id: &str,
        parent_id: Option<TransformId>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_transform_parent")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "set_transform_parent",
                vec![transform_id.into()],
            )));
        };
        transform.parent_id = parent_id;
        let result = self.document().replace_transform(transform);
        self.record(result, "set_transform_parent", transform_id)
    }
    pub fn set_transform_part(
        &mut self,
        transform_id: &str,
        part_id: Option<PartId>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_transform_part")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "set_transform_part",
                vec![transform_id.into()],
            )));
        };
        transform.part_id = part_id;
        let result = self.document().replace_transform(transform);
        self.record(result, "set_transform_part", transform_id)
    }
    pub fn update_rotation(
        &mut self,
        transform_id: &str,
        rotation: RotationTransform,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_rotation")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "update_rotation",
                vec![transform_id.into()],
            )));
        };
        if transform.rotation().is_none() {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("INVALID_TRANSFORM_TYPE", "Transform is not a rotation"),
                "update_rotation",
                vec![transform_id.into()],
            )));
        }
        transform.data = TransformData::Rotation(rotation);
        let result = self.document().replace_transform(transform);
        self.record(result, "update_rotation", transform_id)
    }
    pub fn update_warp_points(
        &mut self,
        transform_id: &str,
        points: Vec<Vec2>,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_warp_points")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "update_warp_points",
                vec![transform_id.into()],
            )));
        };
        let Some(warp) = transform.warp_mut() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("INVALID_TRANSFORM_TYPE", "Transform is not a warp"),
                "update_warp_points",
                vec![transform_id.into()],
            )));
        };
        warp.points = points;
        let result = self.document().replace_transform(transform);
        self.record(result, "update_warp_points", transform_id)
    }
    pub fn create_mesh(&mut self, mesh: Mesh) -> Result<(), SdkError> {
        self.ensure_active("create_mesh")?;
        let id = mesh.id.clone();
        let result = self.document().create_mesh(mesh);
        self.record(result, "create_mesh", &id)
    }
    pub fn replace_mesh(&mut self, mesh: Mesh) -> Result<(), SdkError> {
        self.ensure_active("replace_mesh")?;
        let id = mesh.id.clone();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "replace_mesh", &id)
    }
    /// Changes the mesh's deform parent while retaining source positions as local coordinates.
    pub fn set_deform_parent(&mut self, mesh_id: &str, transform_id: &str) -> Result<(), SdkError> {
        self.ensure_active("set_deform_parent")?;
        let Some(mut mesh) = self.document().get_mesh(mesh_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "set_deform_parent",
                vec![mesh_id.into()],
            )));
        };
        mesh.deformer_id = transform_id.into();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "set_deform_parent", mesh_id)
    }
    pub fn set_mesh_part(&mut self, mesh_id: &str, part_id: &str) -> Result<(), SdkError> {
        self.ensure_active("set_mesh_part")?;
        let Some(mut mesh) = self.document().get_mesh(mesh_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "set_mesh_part",
                vec![mesh_id.into()],
            )));
        };
        mesh.part_id = part_id.into();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "set_mesh_part", mesh_id)
    }
    pub fn rename_mesh(&mut self, id: &str, name: impl Into<String>) -> Result<(), SdkError> {
        self.ensure_active("rename_mesh")?;
        let result = self.document().rename_mesh(id, name.into());
        self.record(result, "rename_mesh", id)
    }
    pub fn update_mesh_properties(
        &mut self,
        id: &str,
        properties: MeshProperties,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_mesh_properties")?;
        let Some(mut mesh) = self.document().get_mesh(id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "update_mesh_properties",
                vec![id.into()],
            )));
        };
        mesh.texture_asset_id = properties.texture_asset_id;
        mesh.appearance = properties.appearance;
        mesh.draw_order = properties.draw_order;
        mesh.blend_mode = properties.blend_mode;
        mesh.enabled = properties.enabled;
        mesh.double_sided = properties.double_sided;
        mesh.inverted_mask = properties.inverted_mask;
        mesh.masks = properties.masks;
        let result = self.document().replace_mesh(mesh);
        self.record(result, "update_mesh_properties", id)
    }
    pub fn replace_topology(
        &mut self,
        source: &GeometrySnapshot,
        replacement: TopologyReplacement,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_topology")?;
        if source.version != self.before || source.mesh_id != replacement.mesh.id {
            let mut error = SdkError::new(
                "STALE_TOPOLOGY",
                "Topology snapshot is not from this edit's starting version and mesh",
                "replace_topology",
            );
            error.object_ids = vec![source.mesh_id.clone(), replacement.mesh.id.clone()];
            error.expected_version = Some(Box::new(source.version));
            error.actual_version = Some(Box::new(self.before));
            return Err(self.abort_with(error));
        }
        let id = replacement.mesh.id.clone();
        let result = self.document().replace_mesh_topology(
            replacement.mesh,
            replacement.binding,
            replacement.blend_bindings,
            replacement.glues,
            replacement.vertex_mapping,
        );
        self.record(result, "replace_topology", &id)
    }
    pub fn create_binding(&mut self, binding: MeshBinding) -> Result<(), SdkError> {
        self.ensure_active("create_binding")?;
        let id = binding.id.clone();
        let result = self.document().create_binding(binding);
        self.record(result, "create_binding", &id)
    }
    pub fn replace_binding(&mut self, binding: MeshBinding) -> Result<(), SdkError> {
        self.ensure_active("replace_binding")?;
        let id = binding.id.clone();
        let result = self.document().replace_binding(binding);
        self.record(result, "replace_binding", &id)
    }
    pub fn set_mesh_keyform(
        &mut self,
        binding_id: &str,
        form: MeshKeyform,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_mesh_keyform")?;
        let result = self.document().set_mesh_keyform(binding_id, form);
        self.record(result, "set_mesh_keyform", binding_id)
    }
    pub fn create_scene_binding(&mut self, binding: SceneBinding) -> Result<(), SdkError> {
        self.ensure_active("create_scene_binding")?;
        let id = binding.id.clone();
        let result = self.document().create_scene_binding(binding);
        self.record(result, "create_scene_binding", &id)
    }
    pub fn replace_scene_binding(&mut self, binding: SceneBinding) -> Result<(), SdkError> {
        self.ensure_active("replace_scene_binding")?;
        let id = binding.id.clone();
        let result = self.document().replace_scene_binding(binding);
        self.record(result, "replace_scene_binding", &id)
    }
    pub fn set_scene_keyform(
        &mut self,
        binding_id: &str,
        form: SceneKeyform,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_scene_keyform")?;
        let result = self.document().set_scene_keyform(binding_id, form);
        self.record(result, "set_scene_keyform", binding_id)
    }
    pub fn create_blend_key_table(&mut self, table: BlendShapeKeyTable) -> Result<(), SdkError> {
        self.ensure_active("create_blend_key_table")?;
        let id = table.id.clone();
        let result = self.document().create_blend_key_table(table);
        self.record(result, "create_blend_key_table", &id)
    }
    pub fn replace_blend_key_table(&mut self, table: BlendShapeKeyTable) -> Result<(), SdkError> {
        self.ensure_active("replace_blend_key_table")?;
        let id = table.id.clone();
        let result = self.document().replace_blend_key_table(table);
        self.record(result, "replace_blend_key_table", &id)
    }
    pub fn create_blend_constraint(
        &mut self,
        constraint: BlendShapeConstraint,
    ) -> Result<(), SdkError> {
        self.ensure_active("create_blend_constraint")?;
        let id = constraint.id.clone();
        let result = self.document().create_blend_constraint(constraint);
        self.record(result, "create_blend_constraint", &id)
    }
    pub fn replace_blend_constraint(
        &mut self,
        constraint: BlendShapeConstraint,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_blend_constraint")?;
        let id = constraint.id.clone();
        let result = self.document().replace_blend_constraint(constraint);
        self.record(result, "replace_blend_constraint", &id)
    }
    pub fn create_blend_binding(&mut self, binding: BlendShapeBinding) -> Result<(), SdkError> {
        self.ensure_active("create_blend_binding")?;
        let id = binding.id.clone();
        let result = self.document().create_blend_binding(binding);
        self.record(result, "create_blend_binding", &id)
    }
    pub fn replace_blend_binding(&mut self, binding: BlendShapeBinding) -> Result<(), SdkError> {
        self.ensure_active("replace_blend_binding")?;
        let id = binding.id.clone();
        let result = self.document().replace_blend_binding(binding);
        self.record(result, "replace_blend_binding", &id)
    }
    pub fn create_glue(&mut self, glue: Glue) -> Result<(), SdkError> {
        self.ensure_active("create_glue")?;
        let id = glue.id.clone();
        let result = self.document().create_glue(glue);
        self.record(result, "create_glue", &id)
    }
    pub fn replace_glue(&mut self, glue: Glue) -> Result<(), SdkError> {
        self.ensure_active("replace_glue")?;
        let id = glue.id.clone();
        let result = self.document().replace_glue(glue);
        self.record(result, "replace_glue", &id)
    }
    pub fn create_offscreen(&mut self, offscreen: Offscreen) -> Result<(), SdkError> {
        self.ensure_active("create_offscreen")?;
        let id = offscreen.id.clone();
        let result = self.document().create_offscreen(offscreen);
        self.record(result, "create_offscreen", &id)
    }
    pub fn replace_offscreen(&mut self, offscreen: Offscreen) -> Result<(), SdkError> {
        self.ensure_active("replace_offscreen")?;
        let id = offscreen.id.clone();
        let result = self.document().replace_offscreen(offscreen);
        self.record(result, "replace_offscreen", &id)
    }
    pub fn replace_part_binding_with_offscreen(
        &mut self,
        binding: SceneBinding,
        offscreen: Offscreen,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_part_binding_with_offscreen")?;
        let id = binding.id.clone();
        let result = self
            .document()
            .replace_part_binding_with_offscreen(binding, offscreen);
        self.record(result, "replace_part_binding_with_offscreen", &id)
    }
    pub fn update_positions(
        &mut self,
        mesh_id: &str,
        vertex_ids: &[VertexId],
        positions: &[Vec2],
    ) -> Result<(), SdkError> {
        self.ensure_active("update_positions")?;
        let result = self
            .document()
            .set_vertex_positions(mesh_id, vertex_ids, positions);
        self.record(result, "update_positions", mesh_id)
    }
    pub fn commit(mut self) -> Result<EditReceipt, SdkError> {
        self.ensure_active("commit")?;
        let candidate = self.candidate.take().expect("edit candidate exists");
        if let Some(issue) = candidate.validate_structure().into_iter().next() {
            return Err(SdkError::from_status(
                issue.status,
                "commit",
                vec![issue.object_id],
            ));
        }
        let identity_keys: HashSet<_> = self
            .erased_keys
            .iter()
            .filter(|key| object_exists_in(&candidate, key.kind, &key.id))
            .cloned()
            .collect();
        let identity_changed = !identity_keys.is_empty();
        let changed = identity_changed || !self.session.project.document().same_content(&candidate);
        let before_keys = changed.then(|| self.session.object_keys());
        let previous = changed.then(|| self.session.project.document().checkpoint());
        let mut evict_count = 0usize;
        if let Some(checkpoint) = &previous {
            let limits = self.session.history_limits;
            let candidate_bytes = candidate.estimated_content_bytes();
            let entry_bytes = checkpoint.estimated_bytes()
                + self.label.capacity()
                + std::mem::size_of::<String>()
                + identity_keys.capacity() * std::mem::size_of::<ObjectKey>()
                + identity_keys
                    .iter()
                    .map(|key| key.id.capacity())
                    .sum::<usize>();
            let redo_bytes: usize = self
                .session
                .redo
                .iter()
                .map(HistoryEntry::estimated_bytes)
                .sum();
            let mut projected_bytes = candidate_bytes
                .saturating_add(entry_bytes)
                .saturating_add(redo_bytes)
                .saturating_add(
                    self.session
                        .done
                        .iter()
                        .map(HistoryEntry::estimated_bytes)
                        .sum::<usize>(),
                );
            let mut projected_steps = self.session.done.len() + 1;
            while (projected_bytes > limits.max_bytes || projected_steps > limits.max_steps)
                && evict_count < self.session.done.len()
            {
                projected_bytes = projected_bytes
                    .saturating_sub(self.session.done[evict_count].estimated_bytes());
                projected_steps -= 1;
                evict_count += 1;
            }
            if projected_bytes > limits.max_bytes || projected_steps > limits.max_steps {
                return Err(SdkError::new(
                    "HISTORY_LIMIT_EXCEEDED",
                    "Edit cannot fit the history budget",
                    "commit",
                ));
            }
        }
        self.session
            .project
            .publish_authoring_candidate(candidate, self.kind, identity_changed)
            .map_err(|s| SdkError::from_status(s, "commit", self.object_ids.clone()))?;
        if changed {
            self.session
                .preview
                .retain_parameters(self.session.project.document());
        }
        if let Some(keys) = &before_keys {
            self.session.refresh_incarnations(keys, &identity_keys);
        }
        if let Some(checkpoint) = previous {
            for _ in 0..evict_count {
                self.session.done.pop_front();
            }
            self.session.done.push_back(HistoryEntry {
                label: self.label.clone(),
                checkpoint,
                identity_keys,
            });
            self.session.redo.clear();
        }
        let receipt = EditReceipt {
            label: self.label.clone(),
            before: self.before,
            after: self.session.version(),
            kind: if changed { self.kind } else { ChangeKind::None },
            object_ids: if changed {
                std::mem::take(&mut self.object_ids)
            } else {
                Vec::new()
            },
            changed,
        };
        if changed {
            self.session.events.push(receipt.clone());
        }
        Ok(receipt)
    }
}

fn merge_kind(a: ChangeKind, b: ChangeKind) -> ChangeKind {
    use ChangeKind::*;
    match (a, b) {
        (Structure, _) | (_, Structure) => Structure,
        (Resources, _) | (_, Resources) => Resources,
        (Positions, _) | (_, Positions) => Positions,
        (Metadata, _) | (_, Metadata) => Metadata,
        _ => None,
    }
}

/// Read a PNG once and return a validated, absolute-path resource description.
/// File publication remains an explicit later project operation.
pub fn prepare_png_asset(id: &str, name: &str, path: &Path) -> Result<ImageAsset, SdkError> {
    let path = fs::canonicalize(path)
        .map_err(|e| SdkError::new("RESOURCE_IO", &e.to_string(), "prepare_png_asset"))?;
    let bytes = fs::read(&path)
        .map_err(|e| SdkError::new("RESOURCE_IO", &e.to_string(), "prepare_png_asset"))?;
    let data = decode_png(&bytes)
        .map_err(|s| SdkError::from_status(s, "prepare_png_asset", vec![id.into()]))?;
    let source = path.to_str().ok_or_else(|| {
        SdkError::new(
            "INVALID_PATH",
            "PNG path cannot be represented as a document source string",
            "prepare_png_asset",
        )
    })?;
    Ok(ImageAsset {
        id: id.into(),
        name: name.into(),
        source: source.into(),
        width: data.width,
        height: data.height,
        sha256: data.sha256,
    })
}

/// Positions are source canvas pixels for a root mesh. UVs follow core source
/// convention and are never flipped in the document.
pub fn rectangle_mesh(
    id: &str,
    name: &str,
    asset_id: &str,
    min: Vec2,
    max: Vec2,
) -> Result<Mesh, SdkError> {
    if !min.x.is_finite()
        || !min.y.is_finite()
        || !max.x.is_finite()
        || !max.y.is_finite()
        || min.x >= max.x
        || min.y >= max.y
    {
        return Err(SdkError::new(
            "INVALID_RECTANGLE",
            "Finite min must be below max",
            "rectangle_mesh",
        ));
    }
    Ok(Mesh {
        id: id.into(),
        name: name.into(),
        texture_asset_id: asset_id.into(),
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        ..Mesh::default()
    })
}
