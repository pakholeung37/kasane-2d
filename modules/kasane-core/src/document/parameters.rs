use super::*;
use crate::types::{ChangeKind, EditResult, Parameter, Status};

impl Document {
    pub fn parameter_order(&self) -> &[String] {
        &self.parameter_order
    }

    pub fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.parameters.get(id)
    }

    pub(super) fn validate_parameter(&self, p: &Parameter) -> Status {
        if !valid_uuid(&p.id) {
            return Status::error(
                "INVALID_ID",
                format!("{}: parameter requires a canonical UUID", p.id),
            );
        }
        if p.runtime_id.is_empty() {
            return Status::error("INVALID_ID", format!("{}.runtime_id is empty", p.id));
        }
        for (id, other) in &self.parameters {
            if id != &p.id && other.runtime_id == p.runtime_id {
                return Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", p.id, id),
                );
            }
        }
        if !p.minimum.is_finite()
            || !p.maximum.is_finite()
            || !p.default_value.is_finite()
            || !(p.maximum - p.minimum).is_finite()
            || p.minimum >= p.maximum
            || p.default_value < p.minimum
            || p.default_value > p.maximum
        {
            return Status::error(
                "INVALID_PARAMETER",
                format!(
                    "{}: require finite minimum < maximum and default in range",
                    p.id
                ),
            );
        }
        if p.decimal_places < 0 || p.decimal_places > 9 {
            return Status::error(
                "INVALID_PARAMETER",
                format!("{}.decimal_places must be 0..9", p.id),
            );
        }
        Status::ok()
    }

    pub fn create_parameter(&mut self, mut p: Parameter) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.contains_id(&p.id) {
            return self.failed(Status::error("DUPLICATE_ID", &p.id));
        }
        if p.runtime_id.is_empty() {
            p.runtime_id = p.id.clone();
        }
        let s = self.validate_parameter(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = p.id.clone();
        self.parameters.insert(id.clone(), p);
        self.parameter_order.push(id.clone());
        self.changed(ChangeKind::Structure, Vec::new(), vec![id])
    }

    pub fn replace_parameter(&mut self, p: Parameter) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.parameters.contains_key(&p.id) {
            return self.failed(Status::error("MISSING_PARAMETER", &p.id));
        }
        let s = self.validate_parameter(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        // Validate dependent keyforms against the proposed range before committing.
        let mut candidate = self.clone();
        candidate.parameters.insert(p.id.clone(), p.clone());
        let mut meshes = Vec::new();
        for binding in self.bindings.values() {
            if binding.axes.iter().any(|axis| axis.parameter_id == p.id) {
                let status = candidate.canonicalize_binding(&mut binding.clone());
                if !status.is_ok() {
                    return self.failed(status);
                }
                meshes.push(binding.mesh_id.clone());
            }
        }
        for binding in self.scene_bindings.values() {
            if binding.axes.iter().any(|axis| axis.parameter_id == p.id) {
                let status = candidate.canonicalize_scene_binding(&mut binding.clone());
                if !status.is_ok() {
                    return self.failed(status);
                }
                meshes = self.mesh_order.clone();
            }
        }
        for glue in self.glues.values() {
            if glue
                .binding
                .as_ref()
                .is_some_and(|b| b.axes.iter().any(|a| a.parameter_id == p.id))
            {
                let status = candidate.validate_glue(glue);
                if !status.is_ok() {
                    return self.failed(status);
                }
                meshes = self.mesh_order.clone();
            }
        }
        for table in self
            .blend_key_tables
            .values()
            .filter(|t| t.parameter_id == p.id)
        {
            let status = candidate.validate_blend_key_table(table);
            if !status.is_ok() {
                return self.failed(status);
            }
            meshes = self.mesh_order.clone();
        }
        for constraint in self
            .blend_constraints
            .values()
            .filter(|c| c.parameter_id == p.id)
        {
            let status = candidate.validate_blend_constraint(constraint);
            if !status.is_ok() {
                return self.failed(status);
            }
            meshes = self.mesh_order.clone();
        }
        meshes.sort();
        meshes.dedup();
        let id = p.id.clone();
        self.parameters.insert(id.clone(), p);
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }
}
