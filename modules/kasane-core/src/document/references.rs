use super::*;
use crate::types::{ChangeKind, EditResult, Status};

impl Document {
    pub fn references_to(&self, id: &str) -> Vec<String> {
        let mut refs = Vec::new();
        for m in &self.mesh_order {
            if self.meshes[m].texture_asset_id == id {
                refs.push(m.clone());
            }
        }
        for b in &self.binding_order {
            let binding = &self.bindings[b];
            let mut refers = binding.mesh_id == id;
            for axis in &binding.axes {
                refers |= axis.parameter_id == id;
            }
            if refers {
                refs.push(b.clone());
            }
        }
        for (key, t) in &self.transforms {
            if t.parent() == id || t.part() == id {
                refs.push(key.clone());
            }
        }
        for (key, p) in &self.parts {
            if p.parent_id == id {
                refs.push(key.clone());
            }
        }
        for (key, m) in &self.meshes {
            if m.part_id == id || m.deformer_id == id || m.masks.iter().any(|mask| mask == id) {
                refs.push(key.clone());
            }
        }
        for (key, b) in &self.scene_bindings {
            let mut refers = b.target_id() == id;
            for a in &b.axes {
                refers |= a.parameter_id == id;
            }
            if refers {
                refs.push(key.clone());
            }
        }
        if let Some(groups) = &self.draw_order_groups {
            for group in groups {
                if group.owner == id {
                    refs.extend(group.items.iter().cloned());
                }
            }
        }
        for (key, kt) in &self.blend_key_tables {
            if kt.parameter_id == id {
                refs.push(key.clone());
            }
        }
        for (key, c) in &self.blend_constraints {
            if c.parameter_id == id {
                refs.push(key.clone());
            }
        }
        for (key, b) in &self.blend_bindings {
            if b.target_id == id
                || b.key_table_id == id
                || b.constraint_ids.iter().any(|cid| cid == id)
            {
                refs.push(key.clone());
            }
        }
        for (key, g) in &self.glues {
            if g.mesh_a_id == id
                || g.mesh_b_id == id
                || g.binding
                    .as_ref()
                    .is_some_and(|b| b.axes.iter().any(|a| a.parameter_id == id))
            {
                refs.push(key.clone());
            }
        }
        for (key, os) in &self.offscreens {
            if os.part_id == id
                || os.masks.iter().any(|mask| mask == id)
                || (!os.part_keyform_indices.is_empty()
                    && self
                        .get_scene_binding(id)
                        .is_some_and(|b| b.target_id() == os.part_id))
            {
                refs.push(key.clone());
            }
        }
        refs.sort();
        refs.dedup();
        refs
    }

    pub fn erase_object(&mut self, id: &str) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if id == self.id || !self.contains_id(id) {
            return self.failed(Status::error("MISSING_OBJECT", id));
        }
        let refs = self.references_to(id);
        if !refs.is_empty() {
            let mut e = self.failed(Status::error(
                "OBJECT_REFERENCED",
                format!("{}: explicitly unbind references first", id),
            ));
            e.referrers = refs;
            return e;
        }
        let mut meshes = Vec::new();
        if let Some(b) = self.get_binding(id) {
            meshes.push(b.mesh_id.clone());
        }
        if self.get_mesh(id).is_some() {
            meshes.push(id.to_string());
        }
        if self.get_blend_binding(id).is_some() {
            // Deformers/Parts affect descendants, and Glue can propagate mesh changes.
            meshes = self.mesh_order.clone();
        }
        if let Some(g) = self.get_glue(id) {
            meshes.push(g.mesh_a_id.clone());
            meshes.push(g.mesh_b_id.clone());
        }
        if self.get_scene_binding(id).is_some()
            || self.get_transform(id).is_some()
            || self.get_part(id).is_some()
        {
            meshes = self.mesh_order.clone();
        }
        if let Some(groups) = &mut self.draw_order_groups {
            groups.retain(|group| group.owner != id);
            for group in groups {
                group.items.retain(|item| item != id);
            }
        }
        self.transforms.remove(id);
        self.parts.remove(id);
        self.scene_bindings.remove(id);
        self.transform_order.retain(|k| k != id);
        self.part_order.retain(|k| k != id);
        self.scene_binding_order.retain(|k| k != id);
        self.assets.remove(id);
        if let Some(mesh) = self.meshes.remove(id) {
            self.mesh_runtime_ids.remove(&mesh.runtime_id);
        }
        self.vertex_slots.remove(id);
        self.parameters.remove(id);
        self.bindings.remove(id);
        self.asset_order.retain(|k| k != id);
        self.mesh_order.retain(|k| k != id);
        self.parameter_order.retain(|k| k != id);
        self.binding_order.retain(|k| k != id);
        self.blend_key_tables.remove(id);
        self.blend_key_table_order.retain(|k| k != id);
        self.blend_constraints.remove(id);
        self.blend_constraint_order.retain(|k| k != id);
        self.blend_bindings.remove(id);
        self.blend_binding_order.retain(|k| k != id);
        self.glues.remove(id);
        self.glue_order.retain(|k| k != id);
        self.offscreens.remove(id);
        self.offscreen_order.retain(|k| k != id);

        meshes.sort();
        meshes.dedup();
        self.changed(ChangeKind::Structure, meshes, vec![id.to_string()])
    }
}
