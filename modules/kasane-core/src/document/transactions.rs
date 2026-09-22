use super::*;
use crate::geometry::validate_positions;
use crate::types::{
    ChangeKind, DocumentEdit, EditResult, Status, Vec2, VertexId, VertexPositionUpdate,
};
use std::collections::{HashMap, HashSet};

impl Document {
    pub fn begin_transaction(&mut self) -> Status {
        if !self.initialized() {
            return Status::error("NOT_INITIALIZED", "Initialize Document first.");
        }
        if self.transaction_active {
            return Status::error("TRANSACTION_ACTIVE", "A transaction is already active.");
        }
        self.transaction_active = true;
        self.staged_edits.clear();
        Status::ok()
    }

    pub fn stage_vertex_positions(&mut self, update: VertexPositionUpdate) -> Status {
        self.stage_edit(DocumentEdit::VertexPositions(update))
    }

    pub fn stage_edit(&mut self, edit: DocumentEdit) -> Status {
        if !self.transaction_active {
            return Status::error("NO_TRANSACTION", "Call begin_transaction first.");
        }
        self.staged_edits.push(edit);
        Status::ok()
    }

    pub fn commit_transaction(&mut self) -> EditResult {
        if !self.transaction_active {
            return self.failed(Status::error("NO_TRANSACTION", "No transaction is active."));
        }
        self.transaction_active = false;
        let staged = std::mem::take(&mut self.staged_edits);

        struct PositionWrite {
            mesh_id: String,
            slot: usize,
            after: Vec2,
        }

        // Resolve and validate every target before the first source write. This
        // is the transaction's prepare phase and intentionally does not clone
        // the document.
        let mut positions = Vec::new();
        let mut names = Vec::new();
        let mut seen_vertices: HashMap<String, HashSet<VertexId>> = HashMap::new();
        let mut seen_names = HashSet::new();
        for edit in staged {
            match edit {
                DocumentEdit::VertexPositions(update) => {
                    let Some(mesh) = self.meshes.get(&update.mesh_id) else {
                        return self.failed(Status::error("MISSING_MESH", "Mesh does not exist."));
                    };
                    if update.vertex_ids.len() != update.positions.len() {
                        return self.failed(Status::error(
                            "INVALID_LENGTH",
                            "IDs and positions must match.",
                        ));
                    }
                    let status = validate_positions(&update.positions);
                    if !status.is_ok() {
                        return self.failed(status);
                    }
                    let mesh_seen = seen_vertices.entry(update.mesh_id.clone()).or_default();
                    for (&vertex, &after) in update.vertex_ids.iter().zip(&update.positions) {
                        if !mesh_seen.insert(vertex) {
                            return self.failed(Status::error(
                                "DUPLICATE_VERTEX",
                                "A transaction cannot write a vertex twice.",
                            ));
                        }
                        let Some(&slot) = self.vertex_slots[&update.mesh_id].get(&vertex) else {
                            return self.failed(Status::error(
                                "MISSING_VERTEX",
                                "Vertex ID does not exist.",
                            ));
                        };
                        if mesh.base_positions[slot] != after {
                            positions.push(PositionWrite {
                                mesh_id: update.mesh_id.clone(),
                                slot,
                                after,
                            });
                        }
                    }
                }
                DocumentEdit::MeshName { mesh_id, name } => {
                    let Some(mesh) = self.meshes.get(&mesh_id) else {
                        return self.failed(Status::error("MISSING_MESH", "Mesh does not exist."));
                    };
                    if !seen_names.insert(mesh_id.clone()) {
                        return self.failed(Status::error(
                            "DUPLICATE_EDIT",
                            "A transaction cannot rename a mesh twice.",
                        ));
                    }
                    if mesh.name != name {
                        names.push((mesh_id, name));
                    }
                }
            }
        }

        let mut delta = crate::history::EditDelta::default();
        let mut changed_meshes = Vec::new();
        let mut changed_mesh_set = HashSet::new();
        let mut object_ids = Vec::new();

        for write in positions {
            let mesh = self.meshes.get_mut(&write.mesh_id).unwrap();
            delta.vertex(
                &write.mesh_id,
                mesh.vertex_ids[write.slot],
                mesh.base_positions[write.slot],
            );
            mesh.base_positions[write.slot] = write.after;
            if changed_mesh_set.insert(write.mesh_id.clone()) {
                changed_meshes.push(write.mesh_id.clone());
            }
        }
        let saw_positions = !changed_meshes.is_empty();
        for (mesh_id, name) in names {
            let mesh = self.meshes.get_mut(&mesh_id).unwrap();
            let before = std::mem::replace(&mut mesh.name, name);
            delta.merge(crate::history::EditDelta::name(&mesh_id, before));
            if changed_mesh_set.insert(mesh_id.clone()) {
                changed_meshes.push(mesh_id.clone());
            }
            object_ids.push(mesh_id);
        }
        let saw_metadata = !object_ids.is_empty();
        if !saw_positions && !saw_metadata {
            return self.failed(Status::ok());
        }
        object_ids.extend(changed_meshes.iter().cloned());
        object_ids.sort();
        object_ids.dedup();
        let kind = if saw_positions && saw_metadata {
            ChangeKind::Structure
        } else if saw_positions {
            ChangeKind::Positions
        } else {
            ChangeKind::Metadata
        };
        let result = self.changed(kind, changed_meshes, object_ids);
        self.receipt.0 = Some(delta);
        result
    }

    pub fn cancel_transaction(&mut self) -> Status {
        if !self.transaction_active {
            return Status::error("NO_TRANSACTION", "No transaction is active.");
        }
        self.transaction_active = false;
        self.staged_edits.clear();
        Status::ok()
    }
}
