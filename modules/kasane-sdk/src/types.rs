//! Public SDK value types and crate-internal history identities.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use kasane_core::document::DocumentCheckpoint;
use kasane_core::{
    Appearance, BlendMode, BlendShapeBinding, ChangeKind, Glue, Mesh, MeshBinding, Status, Vec2,
    VertexId,
};
use kasane_project::{ImportReport, ProjectResult, PsdImportReport};

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
    pub(crate) fn new(code: &str, message: &str, operation: &'static str) -> Self {
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
    pub(crate) fn from_status(status: Status, operation: &'static str, ids: Vec<String>) -> Self {
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
pub struct SaveReceipt {
    pub before: Version,
    pub after: Version,
    pub manifest: PathBuf,
    pub warnings: Vec<String>,
    pub history_warnings: Vec<String>,
    pub durable: bool,
}

#[derive(Debug, Clone)]
pub struct ImportReceipt {
    pub before: Version,
    pub after: Version,
    pub project: ProjectResult,
    pub report: ImportReport,
}

#[derive(Debug, Clone)]
pub struct PsdImportReceipt {
    pub before: Version,
    pub after: Version,
    pub project: ProjectResult,
    pub report: PsdImportReport,
    pub manifest: PathBuf,
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
    CdiParameterGroup,
    CdiCombinedSet,
    Expression,
    Motion,
    Pose,
    Physics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHandle {
    pub(crate) session_id: u64,
    pub(crate) generation: u64,
    pub(crate) object_id: String,
    pub(crate) incarnation: u64,
    pub(crate) kind: ObjectKind,
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
pub(crate) struct ObjectKey {
    pub(crate) kind: ObjectKind,
    pub(crate) id: String,
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

pub(crate) struct HistoryEntry {
    pub(crate) label: String,
    pub(crate) checkpoint: DocumentCheckpoint,
    pub(crate) identity_keys: HashSet<ObjectKey>,
    pub(crate) root: PathBuf,
}

impl HistoryEntry {
    pub(crate) fn estimated_bytes(&self) -> usize {
        self.checkpoint.estimated_bytes()
            + self.label.capacity()
            + std::mem::size_of::<String>()
            + self.identity_keys.capacity() * std::mem::size_of::<ObjectKey>()
            + self
                .identity_keys
                .iter()
                .map(|key| key.id.capacity())
                .sum::<usize>()
            + self.root.as_os_str().len()
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
