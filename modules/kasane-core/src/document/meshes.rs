use super::*;
use crate::geometry::validate_render_mesh;
use crate::types::{
    BlendShapeBinding, BlendShapeTargetKind, ChangeKind, ChangeSet, EditResult, Glue, Mesh,
    MeshBinding, Status, VertexId,
};
use std::collections::{HashMap, HashSet};

impl Document {
    pub fn mesh_order(&self) -> &[String] {
        &self.mesh_order
    }

    pub fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.meshes.get(id)
    }

    pub(super) fn validate_mesh_properties(&self, m: &Mesh) -> Status {
        if !m.part_id.is_empty() && !self.parts.contains_key(&m.part_id) {
            return Status::error("MISSING_PART", format!("{}.part_id", m.id));
        }
        if !m.deformer_id.is_empty() && !self.transforms.contains_key(&m.deformer_id) {
            return Status::error("MISSING_TRANSFORM", format!("{}.deformer_id", m.id));
        }
        let s = validate_appearance(&m.appearance, &m.id);
        if !s.is_ok() {
            return s;
        }
        if let Some(order) = m.draw_order {
            let s = validate_draw_order(order, &m.id);
            if !s.is_ok() {
                return s;
            }
        }
        let mut seen = HashSet::new();
        for mask in &m.masks {
            if mask == &m.id || !seen.insert(mask.clone()) {
                return Status::error("INVALID_MASK", format!("{}.masks", m.id));
            }
            if !self.meshes.contains_key(mask) {
                return Status::error("MISSING_MESH", format!("{}.masks: {}", m.id, mask));
            }
        }
        fn reaches(
            start: &str,
            target: &str,
            meshes: &HashMap<String, Mesh>,
            visited: &mut HashSet<String>,
        ) -> bool {
            if start == target {
                return true;
            }
            if !visited.insert(start.to_string()) {
                return false;
            }
            if let Some(m) = meshes.get(start) {
                for next in &m.masks {
                    if reaches(next, target, meshes, visited) {
                        return true;
                    }
                }
            }
            false
        }
        for mask in &m.masks {
            let mut visited = HashSet::new();
            if reaches(mask, &m.id, &self.meshes, &mut visited) {
                return Status::error("RELATION_CYCLE", format!("{}.masks", m.id));
            }
        }
        Status::ok()
    }

    pub fn create_mesh(&mut self, mut mesh: Mesh) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel the active transaction first.",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error(
                "NOT_INITIALIZED",
                "Initialize Document first.",
            ));
        }
        if !valid_uuid(&mesh.id) {
            return self.failed(Status::error(
                "INVALID_ID",
                "Mesh ID must be a canonical UUID.",
            ));
        }
        if self.contains_id(&mesh.id) {
            return self.failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
        }
        if !self.assets.contains_key(&mesh.texture_asset_id) {
            return self.failed(Status::error(
                "MISSING_ASSET",
                "Texture asset does not exist.",
            ));
        }
        let s = self.validate_mesh_properties(&mesh);
        if !s.is_ok() {
            return self.failed(s);
        }
        if mesh.runtime_id.is_empty() {
            mesh.runtime_id = mesh.id.clone();
        }
        for (id, other) in &self.meshes {
            if other.runtime_id == mesh.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", mesh.id, id),
                ));
            }
        }
        if mesh.vertex_ids.len() != mesh.base_positions.len() {
            return self.failed(Status::error(
                "INVALID_LENGTH",
                "Vertex IDs must match positions.",
            ));
        }
        let mut slots = HashMap::new();
        for (i, &vid) in mesh.vertex_ids.iter().enumerate() {
            if slots.insert(vid, i).is_some() {
                return self.failed(Status::error(
                    "DUPLICATE_VERTEX",
                    "Vertex IDs must be unique within a mesh.",
                ));
            }
        }
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for &vertex in triangle {
                match slots.get(&vertex) {
                    Some(&slot) => indices.push(slot as u32),
                    None => {
                        return self.failed(Status::error(
                            "MISSING_VERTEX",
                            "Triangle references an unknown vertex ID.",
                        ))
                    }
                }
            }
        }
        let s = validate_render_mesh(&mesh.base_positions, &mesh.uvs, &indices);
        if !s.is_ok() {
            return self.failed(s);
        }
        let key = mesh.id.clone();
        self.meshes.insert(key.clone(), mesh);
        self.vertex_slots.insert(key.clone(), slots);
        self.mesh_order.push(key.clone());
        if let Some(groups) = &mut self.draw_order_groups {
            // New meshes join their Part's drawing group, or the nearest
            // grouped ancestor. A flat imported drawing remains flat.
            let mut owner = self.meshes[&key].part_id.clone();
            while !groups.iter().any(|g| g.owner == owner) {
                owner = self.parts[&owner].parent_id.clone();
            }
            groups
                .iter_mut()
                .find(|g| g.owner == owner)
                .unwrap()
                .items
                .push(key.clone());
        }
        self.changed(ChangeKind::Structure, vec![key], Vec::new())
    }

    pub fn replace_mesh(&mut self, mesh: Mesh) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let previous = match self.get_mesh(&mesh.id) {
            Some(m) => m,
            None => return self.failed(Status::error("MISSING_MESH", "Mesh does not exist.")),
        };
        if self.binding_for_mesh(&mesh.id).is_some()
            && (previous.vertex_ids != mesh.vertex_ids || previous.triangles != mesh.triangles)
        {
            return self.failed(Status::error(
                "KEYFORMS_REQUIRED",
                format!(
                    "{}: topology replacement requires every keyform and vertex mapping",
                    mesh.id
                ),
            ));
        }
        if previous.vertex_ids != mesh.vertex_ids {
            for b in self.blend_bindings.values() {
                if b.target_id == mesh.id && b.target_kind == BlendShapeTargetKind::Mesh {
                    return self.failed(Status::error(
                        "KEYFORMS_REQUIRED",
                        format!("{}: mesh topology replacement requires updating blend shape delta keyforms", mesh.id),
                    ));
                }
            }
            let new_vids: HashSet<u32> = mesh.vertex_ids.iter().copied().collect();
            for g in self.glues.values() {
                if g.mesh_a_id == mesh.id {
                    for pair in &g.pairs {
                        if !new_vids.contains(&pair.vertex_a) {
                            return self.failed(Status::error(
                                "GLUE_CONFLICT",
                                format!(
                                    "{}: vertex {} used in glue {} is missing from updated mesh",
                                    mesh.id, pair.vertex_a, g.id
                                ),
                            ));
                        }
                    }
                }
                if g.mesh_b_id == mesh.id {
                    for pair in &g.pairs {
                        if !new_vids.contains(&pair.vertex_b) {
                            return self.failed(Status::error(
                                "GLUE_CONFLICT",
                                format!(
                                    "{}: vertex {} used in glue {} is missing from updated mesh",
                                    mesh.id, pair.vertex_b, g.id
                                ),
                            ));
                        }
                    }
                }
            }
        }
        if !self.assets.contains_key(&mesh.texture_asset_id) {
            return self.failed(Status::error(
                "MISSING_ASSET",
                "Texture asset does not exist.",
            ));
        }
        let s = self.validate_mesh_properties(&mesh);
        if !s.is_ok() {
            return self.failed(s);
        }
        for (id, other) in &self.meshes {
            if id != &mesh.id && other.runtime_id == mesh.runtime_id {
                return self.failed(Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", mesh.id, id),
                ));
            }
        }
        if mesh.vertex_ids.len() != mesh.base_positions.len() {
            return self.failed(Status::error(
                "INVALID_LENGTH",
                "Vertex IDs must match positions.",
            ));
        }
        let mut slots = HashMap::new();
        for (i, &vid) in mesh.vertex_ids.iter().enumerate() {
            if slots.insert(vid, i).is_some() {
                return self.failed(Status::error(
                    "DUPLICATE_VERTEX",
                    "Vertex IDs must be unique within a mesh.",
                ));
            }
        }
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for &vertex in triangle {
                match slots.get(&vertex) {
                    Some(&slot) => indices.push(slot as u32),
                    None => {
                        return self.failed(Status::error(
                            "MISSING_VERTEX",
                            "Triangle references an unknown vertex ID.",
                        ))
                    }
                }
            }
        }
        let s = validate_render_mesh(&mesh.base_positions, &mesh.uvs, &indices);
        if !s.is_ok() {
            return self.failed(s);
        }
        let key = mesh.id.clone();
        self.meshes.insert(key.clone(), mesh);
        self.vertex_slots.insert(key.clone(), slots);
        self.changed(ChangeKind::Structure, vec![key], Vec::new())
    }

    /// Replace bound topology and its complete keyform set as one validated edit.
    pub fn replace_mesh_with_keyforms(&mut self, mesh: Mesh, binding: MeshBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let Some(old_binding) = self.binding_for_mesh(&mesh.id) else {
            return self.failed(Status::error("MISSING_BINDING", &mesh.id));
        };
        if binding.id != old_binding.id || binding.mesh_id != mesh.id {
            return self.failed(Status::error(
                "INVALID_BINDING",
                "Preserve the binding and mesh IDs",
            ));
        }
        let mesh_id = mesh.id.clone();
        let binding_id = binding.id.clone();
        let mut candidate = self.clone();
        let removed = candidate.erase_object(&binding_id);
        if !removed.status.is_ok() {
            return self.failed(removed.status);
        }
        let replaced = candidate.replace_mesh(mesh);
        if !replaced.status.is_ok() {
            return self.failed(replaced.status);
        }
        let rebound = candidate.create_binding(binding);
        if !rebound.status.is_ok() {
            return self.failed(rebound.status);
        }
        self.meshes = candidate.meshes;
        self.vertex_slots = candidate.vertex_slots;
        self.bindings = candidate.bindings;
        self.changed(
            ChangeKind::Structure,
            vec![mesh_id.clone()],
            vec![mesh_id, binding_id],
        )
    }

    /// Replace a mesh and every geometry dependency in one revision. Stable object
    /// IDs and collection order are preserved; failure leaves this document untouched.
    pub fn replace_mesh_topology(
        &mut self,
        mesh: Mesh,
        binding: Option<MeshBinding>,
        blend_bindings: Vec<BlendShapeBinding>,
        glues: Vec<Glue>,
        vertex_mapping: HashMap<VertexId, Option<VertexId>>,
    ) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &mesh.id));
        }
        let Some(previous) = self.get_mesh(&mesh.id) else {
            return self.failed(Status::error("MISSING_MESH", &mesh.id));
        };
        let old_ids: HashSet<_> = previous.vertex_ids.iter().copied().collect();
        let new_ids: HashSet<_> = mesh.vertex_ids.iter().copied().collect();
        let mapped: Vec<_> = vertex_mapping.values().flatten().copied().collect();
        if vertex_mapping.keys().copied().collect::<HashSet<_>>() != old_ids
            || mapped.iter().any(|v| !new_ids.contains(v))
            || mapped.iter().copied().collect::<HashSet<_>>().len() != mapped.len()
        {
            return self.failed(Status::error("INVALID_VERTEX_MAPPING", &mesh.id));
        }
        if self.binding_for_mesh(&mesh.id).map(|b| b.id.as_str())
            != binding.as_ref().map(|b| b.id.as_str())
            || binding.as_ref().is_some_and(|b| b.mesh_id != mesh.id)
        {
            return self.failed(Status::error(
                "INVALID_BINDING",
                "Supply the existing ordinary binding",
            ));
        }
        let expected_blends: HashSet<_> = self
            .blend_bindings
            .values()
            .filter(|b| b.target_id == mesh.id)
            .map(|b| b.id.clone())
            .collect();
        let expected_glues: HashSet<_> = self
            .glues_for_mesh(&mesh.id)
            .iter()
            .map(|g| g.id.clone())
            .collect();
        if blend_bindings
            .iter()
            .map(|b| b.id.clone())
            .collect::<HashSet<_>>()
            != expected_blends
            || blend_bindings.len() != expected_blends.len()
            || blend_bindings
                .iter()
                .any(|b| b.target_id != mesh.id || b.target_kind != BlendShapeTargetKind::Mesh)
            || glues.iter().map(|g| g.id.clone()).collect::<HashSet<_>>() != expected_glues
            || glues.len() != expected_glues.len()
        {
            return self.failed(Status::error("INCOMPLETE_DEPENDENCIES", &mesh.id));
        }
        let ordinary_id = binding.as_ref().map(|b| b.id.clone());
        let mut candidate = self.clone();
        if let Some(b) = &binding {
            candidate.bindings.remove(&b.id);
        }
        for id in &expected_blends {
            candidate.blend_bindings.remove(id);
        }
        for id in &expected_glues {
            candidate.glues.remove(id);
        }
        // Remove only temporary map entries; order arrays are restored on commit.
        candidate
            .binding_order
            .retain(|id| candidate.bindings.contains_key(id));
        candidate.lookup.take();
        let result = candidate.replace_mesh(mesh);
        if !result.status.is_ok() {
            return self.failed(result.status);
        }
        if let Some(mut b) = binding {
            let status = candidate.canonicalize_binding(&mut b);
            if !status.is_ok() {
                return self.failed(status);
            }
            candidate.bindings.insert(b.id.clone(), b);
        }
        for b in blend_bindings {
            let status = candidate.validate_blend_binding(&b);
            if !status.is_ok() {
                return self.failed(status);
            }
            candidate.blend_bindings.insert(b.id.clone(), b);
            candidate.lookup.take();
        }
        for g in glues {
            let status = candidate.validate_glue(&g);
            if !status.is_ok() {
                return self.failed(status);
            }
            // Reuse replacement validation, including runtime-ID uniqueness.
            candidate
                .glues
                .insert(g.id.clone(), self.glues[&g.id].clone());
            let result = candidate.replace_glue(g);
            if !result.status.is_ok() {
                return self.failed(result.status);
            }
        }
        self.meshes = candidate.meshes;
        self.vertex_slots = candidate.vertex_slots;
        self.bindings = candidate.bindings;
        self.blend_bindings = candidate.blend_bindings;
        self.glues = candidate.glues;
        self.changed(
            ChangeKind::Structure,
            self.mesh_order.clone(),
            result
                .changes
                .mesh_ids
                .into_iter()
                .chain(ordinary_id)
                .chain(expected_blends)
                .chain(expected_glues)
                .collect(),
        )
    }

    pub(crate) fn vertex_slot(&self, mesh: &str, vertex: VertexId) -> Option<usize> {
        self.vertex_slots.get(mesh)?.get(&vertex).copied()
    }

    pub fn render_indices(&self, id: &str) -> Result<Vec<u32>, Status> {
        let mut out = Vec::new();
        self.render_indices_into(id, &mut out)?;
        Ok(out)
    }

    pub fn render_indices_into(&self, id: &str, out: &mut Vec<u32>) -> Result<(), Status> {
        let mesh = self
            .get_mesh(id)
            .ok_or_else(|| Status::error("MISSING_MESH", "Mesh does not exist."))?;
        let slots = self
            .vertex_slots
            .get(id)
            .ok_or_else(|| Status::error("MISSING_MESH", "Vertex slots missing."))?;
        out.clear();
        out.reserve(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for vertex in triangle {
                out.push(*slots.get(vertex).unwrap() as u32);
            }
        }
        Ok(())
    }

    pub fn rename_mesh(&mut self, id: &str, name: String) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel the active transaction first.",
            ));
        }
        let mesh = match self.meshes.get_mut(id) {
            Some(m) => m,
            None => return self.failed(Status::error("MISSING_MESH", "Mesh does not exist.")),
        };
        if mesh.name == name {
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
        let before = std::mem::replace(&mut mesh.name, name);
        let result = self.changed(ChangeKind::Metadata, vec![id.to_string()], Vec::new());
        self.receipt.0 = Some(crate::history::EditDelta::name(id, before));
        result
    }
}
