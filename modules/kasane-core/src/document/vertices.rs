use super::*;
use crate::geometry::validate_positions;
use crate::types::{
    ChangeKind, ChangeSet, EditResult, Status, Vec2, VertexId, VertexPositionUpdate,
};
use std::collections::{HashMap, HashSet};

impl Document {
    pub fn set_vertex_positions(
        &mut self,
        id: &str,
        vertices: &[VertexId],
        positions: &[Vec2],
    ) -> EditResult {
        let update = VertexPositionUpdate {
            mesh_id: id.to_string(),
            vertex_ids: vertices.to_vec(),
            positions: positions.to_vec(),
        };
        self.apply_vertex_position_updates(&[update])
    }

    pub fn apply_vertex_position_updates(
        &mut self,
        updates: &[VertexPositionUpdate],
    ) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Use commit_transaction for staged edits.",
            ));
        }

        struct PositionDelta {
            mesh_id: String,
            slots: Vec<usize>,
            after: Vec<Vec2>,
        }

        let mut deltas = Vec::new();
        let mut seen: HashMap<String, HashSet<VertexId>> = HashMap::new();
        let mut changed_meshes = Vec::new();
        let mut changed_mesh_set = HashSet::new();

        for update in updates {
            let mesh = match self.meshes.get(&update.mesh_id) {
                Some(m) => m,
                None => return self.failed(Status::error("MISSING_MESH", "Mesh does not exist.")),
            };
            if update.vertex_ids.len() != update.positions.len() {
                return self.failed(Status::error(
                    "INVALID_LENGTH",
                    "IDs and positions must match.",
                ));
            }
            let s = validate_positions(&update.positions);
            if !s.is_ok() {
                return self.failed(s);
            }
            let mut delta = PositionDelta {
                mesh_id: update.mesh_id.clone(),
                slots: Vec::new(),
                after: Vec::new(),
            };
            let lookup = &self.vertex_slots[&update.mesh_id];
            let mesh_seen = seen.entry(update.mesh_id.clone()).or_default();
            for (i, &vertex) in update.vertex_ids.iter().enumerate() {
                if !mesh_seen.insert(vertex) {
                    return self.failed(Status::error(
                        "DUPLICATE_VERTEX",
                        "A transaction cannot write a vertex twice.",
                    ));
                }
                let &slot = match lookup.get(&vertex) {
                    Some(s) => s,
                    None => {
                        return self
                            .failed(Status::error("MISSING_VERTEX", "Vertex ID does not exist."))
                    }
                };
                let old = mesh.base_positions[slot];
                if old == update.positions[i] {
                    continue;
                }
                delta.slots.push(slot);
                delta.after.push(update.positions[i]);
            }
            if !delta.slots.empty_or() {
                if changed_mesh_set.insert(update.mesh_id.clone()) {
                    changed_meshes.push(update.mesh_id.clone());
                }
                deltas.push(delta);
            }
        }

        if deltas.is_empty() {
            return EditResult {
                status: Status::ok(),
                changes: ChangeSet {
                    kind: ChangeKind::None,
                    mesh_ids: Vec::new(),
                    revision: self.revision,
                    object_ids: Vec::new(),
                },
                referrers: Vec::new(),
            };
        }

        let mut history = crate::history::EditDelta::default();
        for delta in deltas {
            let mesh = self.meshes.get_mut(&delta.mesh_id).unwrap();
            for (slot, after) in delta.slots.into_iter().zip(delta.after) {
                history.vertex(
                    &delta.mesh_id,
                    mesh.vertex_ids[slot],
                    mesh.base_positions[slot],
                );
                mesh.base_positions[slot] = after;
            }
        }

        let result = self.changed(ChangeKind::Positions, changed_meshes, Vec::new());
        self.receipt.0 = Some(history);
        result
    }

    pub fn apply_vertex_position_updates_at_revision(
        &mut self,
        updates: &[VertexPositionUpdate],
        expected_revision: u64,
    ) -> EditResult {
        if self.revision != expected_revision {
            return self.failed(Status::error(
                "STALE_REVISION",
                "Document changed since this transaction began.",
            ));
        }
        self.apply_vertex_position_updates(updates)
    }
}
