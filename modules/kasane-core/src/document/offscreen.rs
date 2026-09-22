use super::*;
use crate::types::{ChangeKind, EditResult, Offscreen, Status};

impl Document {
    pub fn offscreen_order(&self) -> &[String] {
        &self.offscreen_order
    }

    pub fn offscreen_count(&self) -> usize {
        self.offscreens.len()
    }

    pub fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.offscreens.get(id)
    }

    pub fn offscreen_for_part(&self, part_id: &str) -> Option<&Offscreen> {
        self.offscreens.values().find(|os| os.part_id == part_id)
    }

    pub fn is_part_ancestor(&self, ancestor_id: &str, part_id: &str) -> bool {
        if ancestor_id == part_id {
            return true;
        }
        let mut cur = part_id;
        while let Some(part) = self.get_part(cur) {
            if part.parent_id.is_empty() {
                break;
            }
            if part.parent_id == ancestor_id {
                return true;
            }
            cur = &part.parent_id;
        }
        false
    }

    pub fn parent_offscreen_for_part(&self, part_id: &str) -> Option<&Offscreen> {
        let mut cur = part_id;
        while let Some(part) = self.get_part(cur) {
            if part.parent_id.is_empty() {
                break;
            }
            if let Some(os) = self.offscreen_for_part(&part.parent_id) {
                return Some(os);
            }
            cur = &part.parent_id;
        }
        None
    }

    pub fn validate_offscreen(&self, os: &Offscreen) -> Status {
        if !valid_uuid(&os.id) {
            return Status::error("INVALID_ID", format!("{}: invalid UUID", os.id));
        }
        if os.part_id.is_empty() || !self.parts.contains_key(&os.part_id) {
            return Status::error(
                "MISSING_PART",
                format!("{}: owner part {} not found", os.id, os.part_id),
            );
        }
        for (other_id, other) in &self.offscreens {
            if other_id != &os.id && other.part_id == os.part_id {
                return Status::error(
                    "DUPLICATE_OWNER",
                    format!(
                        "{}: part {} already owned by offscreen {}",
                        os.id, os.part_id, other_id
                    ),
                );
            }
        }
        for mask_id in &os.masks {
            if !self.meshes.contains_key(mask_id) {
                return Status::error(
                    "MISSING_MESH",
                    format!("{}: mask mesh {} not found", os.id, mask_id),
                );
            }
        }
        for kf in &os.keyforms {
            if !kf.opacity.is_finite() {
                return Status::error("INVALID_OPACITY", format!("{}: non-finite opacity", os.id));
            }
            if let Some(m) = kf.multiply {
                for c in m {
                    if !c.is_finite() {
                        return Status::error(
                            "INVALID_COLOR",
                            format!("{}: non-finite color", os.id),
                        );
                    }
                }
            }
            if let Some(s) = kf.screen {
                for c in s {
                    if !c.is_finite() {
                        return Status::error(
                            "INVALID_COLOR",
                            format!("{}: non-finite color", os.id),
                        );
                    }
                }
            }
        }
        let klen = self
            .binding_for_scene(&os.part_id)
            .map_or(1, |binding| binding.track.len());
        if !os.part_keyform_indices.is_empty() && os.part_keyform_indices.len() != klen {
            return Status::error(
                "INVALID_LENGTH",
                format!(
                    "{}: part_keyform_indices len ({}) != part keyforms len ({})",
                    os.id,
                    os.part_keyform_indices.len(),
                    klen
                ),
            );
        }
        for &idx in &os.part_keyform_indices {
            if idx >= 0 && (idx as usize) >= os.keyforms.len() {
                return Status::error(
                    "INDEX_OUT_OF_BOUNDS",
                    format!(
                        "{}: keyform index {} exceeds offscreen keyforms len {}",
                        os.id,
                        idx,
                        os.keyforms.len()
                    ),
                );
            }
        }
        Status::ok()
    }

    pub fn create_offscreen(&mut self, offscreen: Offscreen) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &offscreen.id));
        }
        if self.contains_id(&offscreen.id) {
            return self.failed(Status::error("DUPLICATE_ID", &offscreen.id));
        }
        for (other_id, other) in &self.offscreens {
            if other.runtime_id == offscreen.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", offscreen.id, other_id),
                ));
            }
        }
        let s = self.validate_offscreen(&offscreen);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = offscreen.id.clone();
        let part_id = offscreen.part_id.clone();
        self.offscreens.insert(id.clone(), offscreen);
        self.offscreen_order.push(id.clone());
        self.changed(ChangeKind::Structure, Vec::new(), vec![id, part_id])
    }

    pub fn replace_offscreen(&mut self, offscreen: Offscreen) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &offscreen.id));
        }
        if !self.offscreens.contains_key(&offscreen.id) {
            return self.failed(Status::error("MISSING_OBJECT", &offscreen.id));
        }
        for (other_id, other) in &self.offscreens {
            if other_id != &offscreen.id && other.runtime_id == offscreen.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", offscreen.id, other_id),
                ));
            }
        }
        let s = self.validate_offscreen(&offscreen);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = offscreen.id.clone();
        let part_id = offscreen.part_id.clone();
        self.offscreens.insert(id.clone(), offscreen);
        self.changed(ChangeKind::Structure, Vec::new(), vec![id, part_id])
    }
}
