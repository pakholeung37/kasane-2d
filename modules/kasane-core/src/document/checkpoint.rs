use super::{ContentRef, Document, DocumentContent};
use crate::types::{ChangeKind, ImageAsset, Status};
use std::collections::HashMap;

mod size;

/// Persistent document content, without revisions, saved state, or derived caches.
/// A checkpoint may be exchanged with a document to implement full-content undo.
#[derive(Debug, Clone)]
pub struct DocumentCheckpoint(DocumentContent);

impl DocumentCheckpoint {
    /// Estimated heap and inline bytes owned by persistent content. This is a
    /// capacity-aware budget estimate, not a process RSS measurement.
    pub fn estimated_bytes(&self) -> usize {
        size::estimated_content_bytes(self.0.content_ref())
    }

    /// Read-only resource descriptions for SDK history relocation.
    pub fn assets(&self) -> Vec<ImageAsset> {
        self.0
            .asset_order
            .iter()
            .filter_map(|id| self.0.assets.get(id).cloned())
            .collect()
    }

    /// Rewrite only a checkpoint asset's storage location and verified hash.
    /// The complete expected descriptor prevents an ID-only relocation.
    pub fn relocate_asset_storage(
        &mut self,
        expected: &ImageAsset,
        source: String,
        sha256: String,
    ) -> bool {
        let Some(asset) = self.0.assets.get_mut(&expected.id) else {
            return false;
        };
        if asset != expected {
            return false;
        }
        asset.source = source;
        asset.sha256 = sha256;
        true
    }
}

impl Document {
    pub fn estimated_content_bytes(&self) -> usize {
        size::estimated_content_bytes(self.content_ref())
    }
    pub fn checkpoint(&self) -> DocumentCheckpoint {
        DocumentCheckpoint(self.content())
    }

    /// Build an isolated edit candidate. Its saved baseline and evaluation caches
    /// are deliberately absent; only `publish_candidate` can commit it.
    pub fn fork_candidate(&self) -> Document {
        let mut candidate = Document::new();
        let mut content = self.checkpoint();
        candidate.swap_checkpoint(&mut content, true);
        candidate.revision = self.revision;
        candidate.evaluation_revision = self.evaluation_revision;
        candidate
    }

    /// Publish a candidate as one revision while retaining this document's saved
    /// baseline. Callers must keep candidate mutation isolated until this point.
    pub fn publish_candidate(
        &mut self,
        candidate: Document,
        declared_kind: ChangeKind,
        identity_changed: bool,
    ) -> Result<bool, Status> {
        if self.transaction_active || candidate.transaction_active || candidate.batch_build_active {
            return Err(Status::error("EDIT_ACTIVE", "Finish the active edit first"));
        }
        if self.id != candidate.id {
            return Err(Status::error(
                "DIFFERENT_DOCUMENT",
                "Candidate document ID differs",
            ));
        }
        if !identity_changed && self.same_content(&candidate) {
            return Ok(false);
        }
        // Preserve geometry caches only when all persistent changes are known
        // display metadata; identity and runtime inputs must still invalidate.
        let metadata_only = if !identity_changed && declared_kind == ChangeKind::Metadata {
            self.same_content_except_visual_metadata(candidate.content_ref())
        } else {
            false
        };
        let mut candidate = candidate;
        let mut content = DocumentCheckpoint(DocumentContent::default());
        candidate.swap_checkpoint(&mut content, true);
        self.swap_checkpoint(&mut content, !metadata_only);
        self.advance_checkpoint_revision(!metadata_only);
        Ok(true)
    }

    fn same_content_except_visual_metadata(&self, content: ContentRef<'_>) -> bool {
        let original = self.content_ref();
        // Compare borrowed data directly: metadata commits must not clone mesh
        // geometry merely to decide whether the prepared frame remains valid.
        original.id == content.id
            && original.canvas == content.canvas
            && original.assets == content.assets
            && original.asset_order == content.asset_order
            && original.part_order == content.part_order
            && original.transform_order == content.transform_order
            && original.transforms == content.transforms
            && original.mesh_order == content.mesh_order
            && original.parameter_order == content.parameter_order
            && original.bindings == content.bindings
            && original.binding_order == content.binding_order
            && original.scene_bindings == content.scene_bindings
            && original.scene_binding_order == content.scene_binding_order
            && original.draw_order_groups == content.draw_order_groups
            && original.blend_key_tables == content.blend_key_tables
            && original.blend_key_table_order == content.blend_key_table_order
            && original.blend_constraints == content.blend_constraints
            && original.blend_constraint_order == content.blend_constraint_order
            && original.blend_bindings == content.blend_bindings
            && original.blend_binding_order == content.blend_binding_order
            && original.glues == content.glues
            && original.glue_order == content.glue_order
            && original.offscreens == content.offscreens
            && original.offscreen_order == content.offscreen_order
            && original.expressions == content.expressions
            && original.expression_order == content.expression_order
            && original.motions == content.motions
            && original.motion_order == content.motion_order
            && original.motion_groups == content.motion_groups
            && original.pose == content.pose
            && original.physics == content.physics
            && original.missing_attachments == content.missing_attachments
            && original.model3_settings == content.model3_settings
            && original.package_attachments == content.package_attachments
            && original.meshes.len() == content.meshes.len()
            && original.meshes.iter().all(|(id, old)| {
                content.meshes.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.texture_asset_id == new.texture_asset_id
                        && old.vertex_ids == new.vertex_ids
                        && old.base_positions == new.base_positions
                        && old.uvs == new.uvs
                        && old.triangles == new.triangles
                        && old.runtime_id == new.runtime_id
                        && old.part_id == new.part_id
                        && old.deformer_id == new.deformer_id
                        && old.appearance == new.appearance
                        && old.draw_order == new.draw_order
                        && old.blend_mode == new.blend_mode
                        && old.enabled == new.enabled
                        && old.double_sided == new.double_sided
                        && old.inverted_mask == new.inverted_mask
                        && old.masks == new.masks
                        && old.raw_blend_mode == new.raw_blend_mode
                })
            })
            && original.parameters.len() == content.parameters.len()
            && original.parameters.iter().all(|(id, old)| {
                content.parameters.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.runtime_id == new.runtime_id
                        && old.minimum == new.minimum
                        && old.maximum == new.maximum
                        && old.default_value == new.default_value
                        && old.decimal_places == new.decimal_places
                        && old.kind == new.kind
                        && old.repeat == new.repeat
                })
            })
            && original.parts.len() == content.parts.len()
            && original.parts.iter().all(|(id, old)| {
                content.parts.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.runtime_id == new.runtime_id
                        && old.parent_id == new.parent_id
                        && old.enabled == new.enabled
                        && old.draw_order == new.draw_order
                })
            })
    }

    /// Exchange persistent content with a checkpoint from this document, then
    /// advance its revision. A rejected exchange changes neither side.
    pub fn exchange_checkpoint(
        &mut self,
        checkpoint: &mut DocumentCheckpoint,
    ) -> Result<(), Status> {
        if self.transaction_active || self.batch_build_active {
            return Err(Status::error("EDIT_ACTIVE", "Finish the active edit first"));
        }
        if self.id != checkpoint.0.id {
            return Err(Status::error(
                "DIFFERENT_DOCUMENT",
                "Checkpoint document ID differs",
            ));
        }
        let metadata_only = self.same_content_except_visual_metadata(checkpoint.0.content_ref());
        self.swap_checkpoint(checkpoint, !metadata_only);
        self.advance_checkpoint_revision(!metadata_only);
        Ok(())
    }

    /// Swap all persistent content while retaining the saved baseline.
    fn swap_checkpoint(&mut self, checkpoint: &mut DocumentCheckpoint, invalidate: bool) {
        let content = &mut checkpoint.0;
        macro_rules! swap_field {
            ($field:ident) => {
                std::mem::swap(&mut self.$field, &mut content.$field)
            };
        }
        swap_field!(id);
        swap_field!(canvas);
        swap_field!(assets);
        swap_field!(asset_order);
        swap_field!(parts);
        swap_field!(part_order);
        swap_field!(transforms);
        swap_field!(transform_order);
        swap_field!(meshes);
        swap_field!(mesh_order);
        self.mesh_runtime_ids = self
            .meshes
            .iter()
            .map(|(id, mesh)| (mesh.runtime_id.clone(), id.clone()))
            .collect();
        swap_field!(parameters);
        swap_field!(parameter_order);
        swap_field!(bindings);
        swap_field!(binding_order);
        swap_field!(scene_bindings);
        swap_field!(scene_binding_order);
        swap_field!(draw_order_groups);
        swap_field!(blend_key_tables);
        swap_field!(blend_key_table_order);
        swap_field!(blend_constraints);
        swap_field!(blend_constraint_order);
        swap_field!(blend_bindings);
        swap_field!(blend_binding_order);
        swap_field!(glues);
        swap_field!(glue_order);
        swap_field!(offscreens);
        swap_field!(offscreen_order);
        swap_field!(display_info);
        swap_field!(expressions);
        swap_field!(expression_order);
        swap_field!(motions);
        swap_field!(motion_order);
        swap_field!(motion_groups);
        swap_field!(pose);
        swap_field!(physics);
        swap_field!(missing_attachments);
        swap_field!(model3_settings);
        swap_field!(package_attachments);
        self.vertex_slots = self
            .meshes
            .iter()
            .map(|(id, mesh)| {
                let slots: HashMap<_, _> = mesh
                    .vertex_ids
                    .iter()
                    .enumerate()
                    .map(|(slot, vertex)| (*vertex, slot))
                    .collect();
                (id.clone(), slots)
            })
            .collect();
        if invalidate {
            self.lookup.take();
            self.prepared.take();
        }
        self.receipt.0 = None;
    }

    fn advance_checkpoint_revision(&mut self, evaluation_changed: bool) {
        self.revision += 1;
        if evaluation_changed {
            self.evaluation_revision = self.revision;
            self.lookup.take();
            self.prepared.take();
        }
        self.receipt.0 = None;
    }
}
