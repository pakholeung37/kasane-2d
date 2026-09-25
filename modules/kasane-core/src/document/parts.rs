use super::*;
use crate::types::{ChangeKind, EditResult, Part, Status};
use std::collections::{HashMap, HashSet};

impl Document {
    pub fn part_order(&self) -> &[String] {
        &self.part_order
    }

    pub fn get_part(&self, id: &str) -> Option<&Part> {
        self.parts.get(id)
    }

    pub(super) fn validate_part(&self, p: &Part) -> Status {
        if !valid_uuid(&p.id) || p.runtime_id.is_empty() {
            return Status::error("INVALID_ID", &p.id);
        }
        for (id, other) in &self.parts {
            if id != &p.id && other.runtime_id == p.runtime_id {
                return Status::error("DUPLICATE_RUNTIME_ID", &p.id);
            }
        }
        if self.display_info.parts.as_ref().is_some_and(|entries| entries.iter().any(|entry| {
            matches!(entry, CdiPartEntry::Unresolved { runtime_id, .. } if runtime_id == &p.runtime_id)
        })) {
            return Status::error("DUPLICATE_RUNTIME_ID", format!("{}.runtime_id collides with unresolved CDI part", p.id));
        }
        let mut seen = HashSet::new();
        seen.insert(p.id.clone());
        let mut parent_id = p.parent_id.clone();
        while !parent_id.is_empty() {
            if !seen.insert(parent_id.clone()) {
                return Status::error("RELATION_CYCLE", format!("{}.parent_id", p.id));
            }
            match self.get_part(&parent_id) {
                Some(parent) => parent_id = parent.parent_id.clone(),
                None => return Status::error("MISSING_PART", parent_id),
            }
        }
        validate_draw_order(p.draw_order, &p.id)
    }

    pub fn create_part(&mut self, mut p: Part) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &p.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &p.id));
        }
        if self.contains_id(&p.id) {
            return self.failed(Status::error("DUPLICATE_ID", &p.id));
        }
        if p.runtime_id.is_empty() {
            p.runtime_id = p.id.clone();
        }
        let s = self.validate_part(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = p.id.clone();
        self.parts.insert(id.clone(), p);
        self.part_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn replace_part(&mut self, p: Part) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &p.id));
        }
        if !self.parts.contains_key(&p.id) {
            return self.failed(Status::error("MISSING_PART", &p.id));
        }
        let s = self.validate_part(&p);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = p.id.clone();
        self.parts.insert(id.clone(), p);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn sorted_parts(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        fn visit(
            id: &str,
            parts: &HashMap<String, Part>,
            seen: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) {
            if id.is_empty() || !seen.insert(id.to_string()) {
                return;
            }
            if let Some(p) = parts.get(id) {
                visit(&p.parent_id, parts, seen, result);
                result.push(id.to_string());
            }
        }

        for id in &self.part_order {
            visit(id, &self.parts, &mut seen, &mut result);
        }
        result
    }
}
