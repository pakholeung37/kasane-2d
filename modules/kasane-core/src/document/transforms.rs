use super::*;
use crate::geometry::validate_positions;
use crate::types::{ChangeKind, EditResult, Status, Transform};
use std::collections::{HashMap, HashSet};

impl Document {
    pub fn transform_order(&self) -> &[String] {
        &self.transform_order
    }

    pub fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.transforms.get(id)
    }

    pub(super) fn validate_transform(&self, t: &Transform) -> Status {
        if !valid_uuid(&t.id) || t.runtime_id.is_empty() {
            return Status::error("INVALID_ID", &t.id);
        }
        if t.parent_id
            .as_ref()
            .is_some_and(|id| !valid_uuid(id.as_str()))
            || t.part_id
                .as_ref()
                .is_some_and(|id| !valid_uuid(id.as_str()))
        {
            return Status::error("INVALID_ID", &t.id);
        }
        for (id, other) in &self.transforms {
            if id != &t.id && other.runtime_id == t.runtime_id {
                return Status::error("DUPLICATE_RUNTIME_ID", &t.id);
            }
        }
        if !t.part_id.is_none() && !self.parts.contains_key(t.part()) {
            return Status::error("MISSING_PART", format!("{}.part_id", t.id));
        }
        let mut seen = HashSet::new();
        seen.insert(t.id.clone());
        let mut parent_id = t.parent().to_owned();
        while !parent_id.is_empty() {
            if !seen.insert(parent_id.clone()) {
                return Status::error("RELATION_CYCLE", format!("{}.parent_id", t.id));
            }
            match self.get_transform(&parent_id) {
                Some(parent) => parent_id = parent.parent().to_owned(),
                None => return Status::error("MISSING_TRANSFORM", parent_id),
            }
        }
        match &t.data {
            crate::TransformData::Rotation(r) => {
                if !r.base_angle.is_finite() {
                    return Status::error("NON_FINITE", format!("{}.base_angle", t.id));
                }
                let status = pose_valid(&r.pose, &t.id);
                if !status.is_ok() {
                    return status;
                }
            }
            crate::TransformData::Warp(w) => {
                if w.rows == 0
                    || w.columns == 0
                    || w.rows > 1024
                    || w.columns > 1024
                    || w.points.len() != ((w.rows + 1) * (w.columns + 1)) as usize
                {
                    return Status::error("INVALID_WARP_GRID", &t.id);
                }
                let status = validate_positions(&w.points);
                if !status.is_ok() {
                    return status;
                }
            }
        }
        validate_appearance(&t.appearance, &t.id)
    }

    pub fn create_transform(&mut self, mut t: Transform) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &t.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &t.id));
        }
        if self.contains_id(&t.id) {
            return self.failed(Status::error("DUPLICATE_ID", &t.id));
        }
        if t.runtime_id.is_empty() {
            t.runtime_id = t.id.clone();
        }
        let s = self.validate_transform(&t);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = t.id.clone();
        self.transforms.insert(id.clone(), t);
        self.transform_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn replace_transform(&mut self, t: Transform) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &t.id));
        }
        let Some(old) = self.transforms.get(&t.id) else {
            return self.failed(Status::error("MISSING_TRANSFORM", &t.id));
        };
        if (self.binding_for_scene(&t.id).is_some()
            || !self.blend_bindings_for_target(&t.id).is_empty())
            && (old.kind() != t.kind()
                || old.warp().map(|w| (w.rows, w.columns)) != t.warp().map(|w| (w.rows, w.columns)))
        {
            return self.failed(Status::error(
                "KEYFORMS_REQUIRED",
                "Remove ordinary and blend shape bindings before changing transform grid/type",
            ));
        }
        let s = self.validate_transform(&t);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = t.id.clone();
        self.transforms.insert(id.clone(), t);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    pub fn sorted_transforms(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        fn visit(
            id: &str,
            transforms: &HashMap<String, Transform>,
            seen: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) {
            if id.is_empty() || !seen.insert(id.to_string()) {
                return;
            }
            if let Some(t) = transforms.get(id) {
                visit(t.parent(), transforms, seen, result);
                result.push(id.to_string());
            }
        }

        for id in &self.transform_order {
            visit(id, &self.transforms, &mut seen, &mut result);
        }
        result
    }
}
