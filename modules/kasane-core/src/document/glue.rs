use super::*;
use crate::types::{ChangeKind, EditResult, Glue, ParameterKind, Status};
use std::collections::HashSet;

impl Document {
    pub fn glue_order(&self) -> &[String] {
        &self.glue_order
    }

    pub fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.glues.get(id)
    }

    pub fn glues_for_mesh(&self, mesh_id: &str) -> Vec<&Glue> {
        self.glue_order
            .iter()
            .filter_map(|id| self.glues.get(id))
            .filter(|g| g.mesh_a_id == mesh_id || g.mesh_b_id == mesh_id)
            .collect()
    }

    pub(super) fn validate_glue(&self, glue: &Glue) -> Status {
        if !valid_uuid(&glue.id) {
            return Status::error("INVALID_ID", &glue.id);
        }
        if glue.mesh_a_id.is_empty() || glue.mesh_b_id.is_empty() {
            return Status::error("INVALID_GLUE", "Mesh IDs cannot be empty");
        }
        if !self.meshes.contains_key(&glue.mesh_a_id) {
            return Status::error("MISSING_MESH", &glue.mesh_a_id);
        }
        if !self.meshes.contains_key(&glue.mesh_b_id) {
            return Status::error("MISSING_MESH", &glue.mesh_b_id);
        }
        let slots_a = &self.vertex_slots[&glue.mesh_a_id];
        let slots_b = &self.vertex_slots[&glue.mesh_b_id];
        for pair in &glue.pairs {
            if !slots_a.contains_key(&pair.vertex_a) {
                return Status::error(
                    "MISSING_VERTEX",
                    format!("{}: vertex {} not in mesh_a", glue.id, pair.vertex_a),
                );
            }
            if !slots_b.contains_key(&pair.vertex_b) {
                return Status::error(
                    "MISSING_VERTEX",
                    format!("{}: vertex {} not in mesh_b", glue.id, pair.vertex_b),
                );
            }
            if !pair.weight_a.is_finite()
                || pair.weight_a < 0.0
                || !pair.weight_b.is_finite()
                || pair.weight_b < 0.0
            {
                return Status::error(
                    "INVALID_WEIGHT",
                    format!("{}: weights must be finite and non-negative", glue.id),
                );
            }
        }
        if !glue.intensity.is_finite() {
            return Status::error(
                "INVALID_INTENSITY",
                format!("{}: intensity must be finite", glue.id),
            );
        }
        if let Some(binding) = &glue.binding {
            if binding.axes.is_empty() || binding.axes.len() > 16 {
                return Status::error("INVALID_BINDING", "Glue requires 1..16 axes");
            }
            let mut total = 1usize;
            let mut seen = HashSet::new();
            for axis in &binding.axes {
                let Some(p) = self.get_parameter(&axis.parameter_id) else {
                    return Status::error("MISSING_PARAMETER", &axis.parameter_id);
                };
                if p.kind != ParameterKind::Normal {
                    return Status::error("INVALID_PARAMETER_KIND", &p.id);
                }
                if !seen.insert(&axis.parameter_id) {
                    return Status::error("DUPLICATE_AXIS", &p.id);
                }
                if axis.keys.is_empty() || axis.keys.len() > i32::MAX as usize / total {
                    return Status::error("INVALID_KEYS", &glue.id);
                }
                total *= axis.keys.len();
                for (i, &key) in axis.keys.iter().enumerate() {
                    if !key.is_finite()
                        || key < p.minimum
                        || key > p.maximum
                        || (i > 0 && key <= axis.keys[i - 1])
                    {
                        return Status::error("INVALID_KEYS", &p.id);
                    }
                }
            }
            if binding.keyforms.len() != total {
                return Status::error("INCOMPLETE_KEYFORMS", &glue.id);
            }
            if binding.keyforms.iter().any(|k| !k.intensity.is_finite()) {
                return Status::error("INVALID_GLUE", "Glue keyform intensity must be finite");
            }
        }
        Status::ok()
    }

    pub fn create_glue(&mut self, glue: Glue) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &glue.id));
        }
        if self.contains_id(&glue.id) {
            return self.failed(Status::error("DUPLICATE_ID", &glue.id));
        }
        for (other_id, other) in &self.glues {
            if other.runtime_id == glue.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", glue.id, other_id),
                ));
            }
        }
        let s = self.validate_glue(&glue);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = glue.id.clone();
        let mesh_a = glue.mesh_a_id.clone();
        let mesh_b = glue.mesh_b_id.clone();
        self.glues.insert(id.clone(), glue);
        self.glue_order.push(id.clone());
        let mut affected = vec![mesh_a.clone()];
        if mesh_a != mesh_b {
            affected.push(mesh_b.clone());
        }
        self.changed(ChangeKind::Structure, affected, vec![id, mesh_a, mesh_b])
    }

    pub fn replace_glue(&mut self, glue: Glue) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &glue.id));
        }
        let prev = match self.glues.get(&glue.id) {
            Some(g) => (g.mesh_a_id.clone(), g.mesh_b_id.clone()),
            None => return self.failed(Status::error("MISSING_GLUE", &glue.id)),
        };
        for (other_id, other) in &self.glues {
            if other_id != &glue.id && other.runtime_id == glue.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", glue.id, other_id),
                ));
            }
        }
        let s = self.validate_glue(&glue);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = glue.id.clone();
        let mesh_a = glue.mesh_a_id.clone();
        let mesh_b = glue.mesh_b_id.clone();
        self.glues.insert(id.clone(), glue);
        let mut affected = vec![mesh_a.clone(), mesh_b.clone(), prev.0, prev.1];
        affected.sort();
        affected.dedup();
        self.changed(ChangeKind::Structure, affected, vec![id, mesh_a, mesh_b])
    }
}
