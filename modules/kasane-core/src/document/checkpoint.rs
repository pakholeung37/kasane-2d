use super::{Document, DocumentContent};
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
        // A declared metadata edit only keeps evaluation caches when the
        // persistent difference is provably limited to mesh display names.
        let metadata_only = !identity_changed
            && declared_kind == ChangeKind::Metadata
            && self.same_content_except_mesh_names(&candidate);
        let mut candidate = candidate;
        let mut content = DocumentCheckpoint(DocumentContent::default());
        candidate.swap_checkpoint(&mut content, true);
        self.swap_checkpoint(&mut content, !metadata_only);
        self.advance_checkpoint_revision(!metadata_only);
        Ok(true)
    }

    fn same_content_except_mesh_names(&self, candidate: &Document) -> bool {
        let mut normalized = candidate.content();
        for (id, mesh) in &mut normalized.meshes {
            let Some(existing) = self.meshes.get(id) else {
                return false;
            };
            mesh.name.clone_from(&existing.name);
        }
        self.content_ref() == normalized.content_ref()
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
        self.swap_checkpoint(checkpoint, true);
        self.advance_checkpoint_revision(true);
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
