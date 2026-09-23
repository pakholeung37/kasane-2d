mod assets;
mod bindings;
mod blendshapes;
mod canvas;
mod checkpoint;
mod glue;
mod meshes;
mod offscreen;
mod parameters;
mod parts;
mod references;
mod structure;
mod transactions;
mod transforms;
mod validation;
mod vertices;

pub use checkpoint::DocumentCheckpoint;
pub use structure::StructureIssue;
pub use validation::*;

use crate::draw_order::DrawOrderGroup;
use crate::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, Canvas, ChangeKind, ChangeSet,
    DocumentEdit, EditResult, Glue, ImageAsset, Mesh, MeshBinding, Offscreen, Parameter, Part,
    SceneBinding, Status, Transform, VertexId,
};
use std::collections::HashMap;
use std::sync::OnceLock;

pub(super) trait EmptyOr {
    fn empty_or(&self) -> bool;
}

impl<T> EmptyOr for Vec<T> {
    fn empty_or(&self) -> bool {
        self.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    id: String,
    canvas: Canvas,
    revision: u64,
    evaluation_revision: u64,
    batch_build_active: bool,
    transaction_active: bool,
    staged_edits: Vec<DocumentEdit>,
    receipt: crate::history::Receipt,

    assets: HashMap<String, ImageAsset>,
    asset_order: Vec<String>,

    parts: HashMap<String, Part>,
    part_order: Vec<String>,

    transforms: HashMap<String, Transform>,
    transform_order: Vec<String>,

    meshes: HashMap<String, Mesh>,
    vertex_slots: HashMap<String, HashMap<VertexId, usize>>,
    mesh_order: Vec<String>,

    parameters: HashMap<String, Parameter>,
    parameter_order: Vec<String>,

    bindings: HashMap<String, MeshBinding>,
    binding_order: Vec<String>,

    scene_bindings: HashMap<String, SceneBinding>,
    scene_binding_order: Vec<String>,
    draw_order_groups: Option<Vec<DrawOrderGroup>>,

    blend_key_tables: HashMap<String, BlendShapeKeyTable>,
    blend_key_table_order: Vec<String>,

    blend_constraints: HashMap<String, BlendShapeConstraint>,
    blend_constraint_order: Vec<String>,

    blend_bindings: HashMap<String, BlendShapeBinding>,
    blend_binding_order: Vec<String>,

    glues: HashMap<String, Glue>,
    glue_order: Vec<String>,

    offscreens: HashMap<String, Offscreen>,
    offscreen_order: Vec<String>,

    saved_content: Option<Box<DocumentContent>>,
    lookup: OnceLock<DocumentLookup>,
    prepared: OnceLock<Result<crate::evaluation::PreparedEvaluation, Status>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DocumentLookup {
    pub(super) mesh_bindings: HashMap<String, String>,
    pub(super) scene_bindings: HashMap<String, String>,
    pub(super) blend_bindings: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DocumentContent {
    id: String,
    canvas: Canvas,
    assets: HashMap<String, ImageAsset>,
    asset_order: Vec<String>,
    parts: HashMap<String, Part>,
    part_order: Vec<String>,
    transforms: HashMap<String, Transform>,
    transform_order: Vec<String>,
    meshes: HashMap<String, Mesh>,
    mesh_order: Vec<String>,
    parameters: HashMap<String, Parameter>,
    parameter_order: Vec<String>,
    bindings: HashMap<String, MeshBinding>,
    binding_order: Vec<String>,
    scene_bindings: HashMap<String, SceneBinding>,
    scene_binding_order: Vec<String>,
    draw_order_groups: Option<Vec<DrawOrderGroup>>,
    blend_key_tables: HashMap<String, BlendShapeKeyTable>,
    blend_key_table_order: Vec<String>,
    blend_constraints: HashMap<String, BlendShapeConstraint>,
    blend_constraint_order: Vec<String>,
    blend_bindings: HashMap<String, BlendShapeBinding>,
    blend_binding_order: Vec<String>,
    glues: HashMap<String, Glue>,
    glue_order: Vec<String>,
    offscreens: HashMap<String, Offscreen>,
    offscreen_order: Vec<String>,
}

#[derive(PartialEq)]
struct ContentRef<'a> {
    id: &'a String,
    canvas: &'a Canvas,
    assets: &'a HashMap<String, ImageAsset>,
    asset_order: &'a Vec<String>,
    parts: &'a HashMap<String, Part>,
    part_order: &'a Vec<String>,
    transforms: &'a HashMap<String, Transform>,
    transform_order: &'a Vec<String>,
    meshes: &'a HashMap<String, Mesh>,
    mesh_order: &'a Vec<String>,
    parameters: &'a HashMap<String, Parameter>,
    parameter_order: &'a Vec<String>,
    bindings: &'a HashMap<String, MeshBinding>,
    binding_order: &'a Vec<String>,
    scene_bindings: &'a HashMap<String, SceneBinding>,
    scene_binding_order: &'a Vec<String>,
    draw_order_groups: &'a Option<Vec<DrawOrderGroup>>,
    blend_key_tables: &'a HashMap<String, BlendShapeKeyTable>,
    blend_key_table_order: &'a Vec<String>,
    blend_constraints: &'a HashMap<String, BlendShapeConstraint>,
    blend_constraint_order: &'a Vec<String>,
    blend_bindings: &'a HashMap<String, BlendShapeBinding>,
    blend_binding_order: &'a Vec<String>,
    glues: &'a HashMap<String, Glue>,
    glue_order: &'a Vec<String>,
    offscreens: &'a HashMap<String, Offscreen>,
    offscreen_order: &'a Vec<String>,
}

impl DocumentContent {
    fn content_ref(&self) -> ContentRef<'_> {
        ContentRef {
            id: &self.id,
            canvas: &self.canvas,
            assets: &self.assets,
            asset_order: &self.asset_order,
            parts: &self.parts,
            part_order: &self.part_order,
            transforms: &self.transforms,
            transform_order: &self.transform_order,
            meshes: &self.meshes,
            mesh_order: &self.mesh_order,
            parameters: &self.parameters,
            parameter_order: &self.parameter_order,
            bindings: &self.bindings,
            binding_order: &self.binding_order,
            scene_bindings: &self.scene_bindings,
            scene_binding_order: &self.scene_binding_order,
            draw_order_groups: &self.draw_order_groups,
            blend_key_tables: &self.blend_key_tables,
            blend_key_table_order: &self.blend_key_table_order,
            blend_constraints: &self.blend_constraints,
            blend_constraint_order: &self.blend_constraint_order,
            blend_bindings: &self.blend_bindings,
            blend_binding_order: &self.blend_binding_order,
            glues: &self.glues,
            glue_order: &self.glue_order,
            offscreens: &self.offscreens,
            offscreen_order: &self.offscreen_order,
        }
    }
}

impl Document {
    pub const SCHEMA_VERSION: u32 = 3;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn initialize(&mut self, id: impl Into<String>, canvas: Canvas) -> Status {
        if self.initialized() {
            return Status::error(
                "ALREADY_INITIALIZED",
                "Create a new Document to open another model.",
            );
        }
        let id_str = id.into();
        if !valid_uuid(&id_str) {
            return Status::error("INVALID_ID", "Use a nonzero canonical lowercase UUID.");
        }
        if !canvas.width.is_finite()
            || !canvas.height.is_finite()
            || canvas.width <= 0.0
            || canvas.height <= 0.0
            || !canvas.origin.x.is_finite()
            || !canvas.origin.y.is_finite()
            || !canvas.pixels_per_unit.is_finite()
            || canvas.pixels_per_unit <= 0.0
        {
            return Status::error(
                "INVALID_CANVAS",
                "Canvas dimensions must be finite and positive.",
            );
        }
        self.id = id_str;
        self.canvas = canvas;
        Status::ok()
    }

    pub fn initialized(&self) -> bool {
        !self.id.is_empty()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn canvas(&self) -> Canvas {
        self.canvas
    }

    /// Last document revision that changed visual inputs. Names do not affect frames.
    pub fn evaluation_revision(&self) -> u64 {
        self.evaluation_revision
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn content_ref(&self) -> ContentRef<'_> {
        ContentRef {
            id: &self.id,
            canvas: &self.canvas,
            assets: &self.assets,
            asset_order: &self.asset_order,
            parts: &self.parts,
            part_order: &self.part_order,
            transforms: &self.transforms,
            transform_order: &self.transform_order,
            meshes: &self.meshes,
            mesh_order: &self.mesh_order,
            parameters: &self.parameters,
            parameter_order: &self.parameter_order,
            bindings: &self.bindings,
            binding_order: &self.binding_order,
            scene_bindings: &self.scene_bindings,
            scene_binding_order: &self.scene_binding_order,
            draw_order_groups: &self.draw_order_groups,
            blend_key_tables: &self.blend_key_tables,
            blend_key_table_order: &self.blend_key_table_order,
            blend_constraints: &self.blend_constraints,
            blend_constraint_order: &self.blend_constraint_order,
            blend_bindings: &self.blend_bindings,
            blend_binding_order: &self.blend_binding_order,
            glues: &self.glues,
            glue_order: &self.glue_order,
            offscreens: &self.offscreens,
            offscreen_order: &self.offscreen_order,
        }
    }

    fn content(&self) -> DocumentContent {
        DocumentContent {
            id: self.id.clone(),
            canvas: self.canvas,
            assets: self.assets.clone(),
            asset_order: self.asset_order.clone(),
            parts: self.parts.clone(),
            part_order: self.part_order.clone(),
            transforms: self.transforms.clone(),
            transform_order: self.transform_order.clone(),
            meshes: self.meshes.clone(),
            mesh_order: self.mesh_order.clone(),
            parameters: self.parameters.clone(),
            parameter_order: self.parameter_order.clone(),
            bindings: self.bindings.clone(),
            binding_order: self.binding_order.clone(),
            scene_bindings: self.scene_bindings.clone(),
            scene_binding_order: self.scene_binding_order.clone(),
            draw_order_groups: self.draw_order_groups.clone(),
            blend_key_tables: self.blend_key_tables.clone(),
            blend_key_table_order: self.blend_key_table_order.clone(),
            blend_constraints: self.blend_constraints.clone(),
            blend_constraint_order: self.blend_constraint_order.clone(),
            blend_bindings: self.blend_bindings.clone(),
            blend_binding_order: self.blend_binding_order.clone(),
            glues: self.glues.clone(),
            glue_order: self.glue_order.clone(),
            offscreens: self.offscreens.clone(),
            offscreen_order: self.offscreen_order.clone(),
        }
    }

    pub fn take_edit_delta(&mut self) -> Option<crate::history::EditDelta> {
        self.receipt.0.take()
    }

    pub(crate) fn remove_unchanged_history_fields(&self, delta: &mut crate::history::EditDelta) {
        use crate::history::{Field, Value};
        delta.values.retain(|key, value| match (key, value) {
            (Field::MeshName(id), Value::Name(name)) => {
                self.get_mesh(id).is_none_or(|m| m.name != *name)
            }
            (Field::Vertex(id, vertex), Value::Position(position)) => {
                self.get_mesh(id).and_then(|m| {
                    self.vertex_slots
                        .get(id)?
                        .get(vertex)
                        .map(|&slot| m.base_positions[slot])
                }) != Some(*position)
            }
            _ => true,
        });
    }

    pub(crate) fn replay_history(&mut self, delta: &mut crate::history::EditDelta) -> EditResult {
        use crate::history::{Field, Value};
        // Validate every target before the first swap; failure cannot partially replay an action.
        for (key, value) in &delta.values {
            let valid = match (key, value) {
                (Field::MeshName(id), Value::Name(_)) => self.meshes.contains_key(id),
                (Field::Vertex(id, vertex), Value::Position(_)) => self
                    .vertex_slots
                    .get(id)
                    .is_some_and(|s| s.contains_key(vertex)),
                _ => false,
            };
            if !valid {
                return self.failed(Status::error(
                    "STALE_HISTORY",
                    "History target is no longer available",
                ));
            }
        }
        self.remove_unchanged_history_fields(delta);
        if delta.values.is_empty() {
            return self.failed(Status::ok());
        }
        let mut meshes = Vec::new();
        let mut positions = false;
        for (key, value) in &mut delta.values {
            match (key, value) {
                (Field::MeshName(id), Value::Name(name)) => {
                    std::mem::swap(&mut self.meshes.get_mut(id).unwrap().name, name);
                    meshes.push(id.clone());
                }
                (Field::Vertex(id, vertex), Value::Position(position)) => {
                    let slot = self.vertex_slots[id][vertex];
                    std::mem::swap(
                        &mut self.meshes.get_mut(id).unwrap().base_positions[slot],
                        position,
                    );
                    positions = true;
                    meshes.push(id.clone());
                }
                _ => unreachable!(),
            }
        }
        meshes.sort();
        meshes.dedup();
        self.changed(
            if positions {
                ChangeKind::Positions
            } else {
                ChangeKind::Metadata
            },
            meshes,
            Vec::new(),
        )
    }

    pub fn same_content(&self, other: &Document) -> bool {
        self.content_ref() == other.content_ref()
    }

    pub fn modified(&self) -> bool {
        match &self.saved_content {
            Some(saved) => self.content_ref() != saved.content_ref(),
            None => self.initialized(),
        }
    }

    pub fn mark_saved(&mut self) {
        self.saved_content = Some(Box::new(self.content()));
    }

    pub fn restore_from(&mut self, source: &Document) {
        let next_rev = self.revision + 1;
        let saved = self.saved_content.clone();
        *self = source.clone();
        self.saved_content = saved;
        self.revision = next_rev;
        self.evaluation_revision = next_rev;
        self.transaction_active = false;
        self.batch_build_active = false;
        self.staged_edits.clear();
    }

    pub fn transaction_active(&self) -> bool {
        self.transaction_active
    }

    /// Begin construction of a new document without publishing a revision and
    /// invalidating derived caches after every inserted object.
    pub fn begin_batch_build(&mut self) -> Status {
        if self.initialized() || self.batch_build_active || self.transaction_active {
            return Status::error(
                "INVALID_BUILD_STATE",
                "Batch construction requires a new document",
            );
        }
        self.batch_build_active = true;
        Status::ok()
    }

    /// Publish a successfully constructed candidate as one document revision.
    pub fn finish_batch_build(&mut self) -> Status {
        if !self.batch_build_active {
            return Status::error("NO_BATCH_BUILD", "Call begin_batch_build first");
        }
        if self.transaction_active || !self.staged_edits.is_empty() {
            return Status::error(
                "INVALID_BUILD_STATE",
                "Commit or cancel the active transaction before finishing the batch",
            );
        }
        if !self.initialized() {
            return Status::error("NOT_INITIALIZED", "Initialize the document first");
        }
        self.batch_build_active = false;
        self.lookup.take();
        self.prepared.take();
        self.receipt.0 = None;
        self.revision = 1;
        self.evaluation_revision = 1;
        Status::ok()
    }

    pub(super) fn mutation_blocked(&self) -> bool {
        self.transaction_active
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.id == id
            || self.assets.contains_key(id)
            || self.parts.contains_key(id)
            || self.transforms.contains_key(id)
            || self.meshes.contains_key(id)
            || self.parameters.contains_key(id)
            || self.bindings.contains_key(id)
            || self.scene_bindings.contains_key(id)
            || self.blend_key_tables.contains_key(id)
            || self.blend_constraints.contains_key(id)
            || self.blend_bindings.contains_key(id)
            || self.glues.contains_key(id)
            || self.offscreens.contains_key(id)
    }

    pub(super) fn failed(&self, status: Status) -> EditResult {
        EditResult {
            status,
            changes: ChangeSet {
                kind: ChangeKind::None,
                mesh_ids: Vec::new(),
                revision: self.revision,
                object_ids: Vec::new(),
            },
            referrers: Vec::new(),
        }
    }

    pub(crate) fn prepared_evaluation(
        &self,
    ) -> Result<&crate::evaluation::PreparedEvaluation, &Status> {
        self.prepared
            .get_or_init(|| crate::evaluation::PreparedEvaluation::new(self))
            .as_ref()
    }

    pub(super) fn changed(
        &mut self,
        kind: ChangeKind,
        mesh_ids: Vec<String>,
        mut object_ids: Vec<String>,
    ) -> EditResult {
        if self.batch_build_active {
            if object_ids.is_empty() {
                object_ids = mesh_ids.clone();
            }
            if kind == ChangeKind::Structure {
                self.lookup.take();
                self.prepared.take();
            }
            return EditResult {
                status: Status::ok(),
                changes: ChangeSet {
                    kind,
                    mesh_ids,
                    revision: self.revision,
                    object_ids,
                },
                referrers: Vec::new(),
            };
        }
        if object_ids.is_empty() {
            object_ids = mesh_ids.clone();
        }
        // References only change with structure edits; vertex drags retain the index.
        if kind == ChangeKind::Structure {
            self.lookup.take();
            self.prepared.take();
        }
        self.revision += 1;
        if kind != ChangeKind::Metadata {
            self.evaluation_revision = self.revision;
        }
        self.receipt.0 = None;
        EditResult {
            status: Status::ok(),
            changes: ChangeSet {
                kind,
                mesh_ids,
                revision: self.revision,
                object_ids,
            },
            referrers: Vec::new(),
        }
    }

    pub(super) fn lookup(&self) -> &DocumentLookup {
        self.lookup.get_or_init(|| {
            let mut lookup = DocumentLookup::default();
            for id in &self.binding_order {
                lookup
                    .mesh_bindings
                    .insert(self.bindings[id].mesh_id.clone(), id.clone());
            }
            for id in &self.scene_binding_order {
                lookup
                    .scene_bindings
                    .insert(self.scene_bindings[id].target_id().to_owned(), id.clone());
            }
            for id in &self.blend_binding_order {
                if let Some(binding) = self.blend_bindings.get(id) {
                    lookup
                        .blend_bindings
                        .entry(binding.target_id.clone())
                        .or_default()
                        .push(id.clone());
                }
            }
            lookup
        })
    }
}
