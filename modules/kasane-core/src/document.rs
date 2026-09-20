use crate::draw_order::{validate_groups, DrawOrderGroup};
use std::collections::{HashMap, HashSet};

use crate::geometry::{validate_positions, validate_render_mesh};
use crate::types::{
    Appearance, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    Canvas, ChangeKind, ChangeSet, DeltaKeyforms, EditResult, Glue, ImageAsset, Mesh,
    MeshBinding, MeshKeyform, Parameter, ParameterKind, Part, RotationPose, SceneBinding,
    SceneKeyform, Status, Transform, TransformKind, Vec2, VertexId, VertexPositionUpdate,
};

pub fn valid_uuid(id: &str) -> bool {
    if id.len() != 36 {
        return false;
    }
    let mut nonzero = false;
    for (i, b) in id.bytes().enumerate() {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            if b != b'-' {
                return false;
            }
        } else {
            if !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b) {
                return false;
            }
            nonzero |= b != b'0';
        }
    }
    nonzero
}

pub fn validate_appearance(a: &Appearance, id: &str) -> Status {
    // MOC3 keyform opacity can exceed 1 (e.g. authored 1.00005).
    // Core interpolates it without clamping; retain that value for roundtrips.
    if !a.opacity.is_finite() || a.opacity < 0.0 {
        return Status::error("INVALID_OPACITY", id);
    }
    for &c in a.multiply.iter().chain(a.screen.iter()) {
        if !c.is_finite() || c < 0.0 || c > 1.0 {
            return Status::error("INVALID_COLOR", id);
        }
    }
    Status::ok()
}

pub fn validate_draw_order(order: f32, id: &str) -> Status {
    if !order.is_finite() || order < -32768.0 || order > 32767.0 {
        return Status::error(
            "INVALID_DRAW_ORDER",
            format!("{}: supported order range -32768..32767", id),
        );
    }
    Status::ok()
}

pub fn pose_valid(p: &RotationPose, id: &str) -> Status {
    if !p.origin.x.is_finite()
        || !p.origin.y.is_finite()
        || !p.angle.is_finite()
        || !p.scale.is_finite()
        || p.scale < 0.0
    {
        return Status::error("INVALID_ROTATION", id);
    }
    Status::ok()
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    id: String,
    canvas: Canvas,
    revision: u64,
    transaction_active: bool,
    staged_updates: Vec<VertexPositionUpdate>,

    assets: HashMap<String, ImageAsset>,
    asset_order: Vec<String>,

    parts: HashMap<String, Part>,
    part_order: Vec<String>,

    transforms: HashMap<String, Transform>,
    transform_order: Vec<String>,

    meshes: HashMap<String, Mesh>,
    vertex_slots: HashMap<String, HashMap<VertexId, usize>>,
    mesh_order: Vec<String>,

    parameters: HashMap<String, Parameter>,
    parameter_order: Vec<String>,

    bindings: HashMap<String, MeshBinding>,
    binding_order: Vec<String>,

    scene_bindings: HashMap<String, SceneBinding>,
    scene_binding_order: Vec<String>,
    draw_order_groups: Option<Vec<DrawOrderGroup>>,

    blend_key_tables: HashMap<String, BlendShapeKeyTable>,
    blend_key_table_order: Vec<String>,

    blend_constraints: HashMap<String, BlendShapeConstraint>,
    blend_constraint_order: Vec<String>,

    blend_bindings: HashMap<String, BlendShapeBinding>,
    blend_binding_order: Vec<String>,

    glues: HashMap<String, Glue>,
    glue_order: Vec<String>,

    saved_content: Option<Box<DocumentContent>>,
}

#[derive(Debug, Clone, PartialEq)]
struct DocumentContent {
    id: String,
    canvas: Canvas,
    assets: HashMap<String, ImageAsset>,
    asset_order: Vec<String>,
    parts: HashMap<String, Part>,
    part_order: Vec<String>,
    transforms: HashMap<String, Transform>,
    transform_order: Vec<String>,
    meshes: HashMap<String, Mesh>,
    mesh_order: Vec<String>,
    parameters: HashMap<String, Parameter>,
    parameter_order: Vec<String>,
    bindings: HashMap<String, MeshBinding>,
    binding_order: Vec<String>,
    scene_bindings: HashMap<String, SceneBinding>,
    scene_binding_order: Vec<String>,
    draw_order_groups: Option<Vec<DrawOrderGroup>>,
    blend_key_tables: HashMap<String, BlendShapeKeyTable>,
    blend_key_table_order: Vec<String>,
    blend_constraints: HashMap<String, BlendShapeConstraint>,
    blend_constraint_order: Vec<String>,
    blend_bindings: HashMap<String, BlendShapeBinding>,
    blend_binding_order: Vec<String>,
    glues: HashMap<String, Glue>,
    glue_order: Vec<String>,
}

impl Document {
    pub const SCHEMA_VERSION: u32 = 3;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn initialize(&mut self, id: impl Into<String>, canvas: Canvas) -> Status {
        if self.initialized() {
            return Status::error(
                "ALREADY_INITIALIZED",
                "Create a new Document to open another model.",
            );
        }
        let id_str = id.into();
        if !valid_uuid(&id_str) {
            return Status::error("INVALID_ID", "Use a nonzero canonical lowercase UUID.");
        }
        if !canvas.width.is_finite()
            || !canvas.height.is_finite()
            || canvas.width <= 0.0
            || canvas.height <= 0.0
            || !canvas.origin.x.is_finite()
            || !canvas.origin.y.is_finite()
            || !canvas.pixels_per_unit.is_finite()
            || canvas.pixels_per_unit <= 0.0
        {
            return Status::error(
                "INVALID_CANVAS",
                "Canvas dimensions must be finite and positive.",
            );
        }
        self.id = id_str;
        self.canvas = canvas;
        Status::ok()
    }

    pub fn initialized(&self) -> bool {
        !self.id.is_empty()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn canvas(&self) -> Canvas {
        self.canvas
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn content(&self) -> DocumentContent {
        DocumentContent {
            id: self.id.clone(),
            canvas: self.canvas,
            assets: self.assets.clone(),
            asset_order: self.asset_order.clone(),
            parts: self.parts.clone(),
            part_order: self.part_order.clone(),
            transforms: self.transforms.clone(),
            transform_order: self.transform_order.clone(),
            meshes: self.meshes.clone(),
            mesh_order: self.mesh_order.clone(),
            parameters: self.parameters.clone(),
            parameter_order: self.parameter_order.clone(),
            bindings: self.bindings.clone(),
            binding_order: self.binding_order.clone(),
            scene_bindings: self.scene_bindings.clone(),
            scene_binding_order: self.scene_binding_order.clone(),
            draw_order_groups: self.draw_order_groups.clone(),
            blend_key_tables: self.blend_key_tables.clone(),
            blend_key_table_order: self.blend_key_table_order.clone(),
            blend_constraints: self.blend_constraints.clone(),
            blend_constraint_order: self.blend_constraint_order.clone(),
            blend_bindings: self.blend_bindings.clone(),
            blend_binding_order: self.blend_binding_order.clone(),
            glues: self.glues.clone(),
            glue_order: self.glue_order.clone(),
        }
    }

    pub fn same_content(&self, other: &Document) -> bool {
        self.content() == other.content()
    }

    pub fn modified(&self) -> bool {
        match &self.saved_content {
            Some(saved) => &self.content() != saved.as_ref(),
            None => self.initialized(),
        }
    }

    pub fn mark_saved(&mut self) {
        self.saved_content = Some(Box::new(self.content()));
    }

    pub fn restore_from(&mut self, source: &Document) {
        let next_rev = self.revision + 1;
        let saved = self.saved_content.clone();
        *self = source.clone();
        self.saved_content = saved;
        self.revision = next_rev;
        self.transaction_active = false;
        self.staged_updates.clear();
    }

    pub fn transaction_active(&self) -> bool {
        self.transaction_active
    }

    fn mutation_blocked(&self) -> bool {
        self.transaction_active
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.id == id
            || self.assets.contains_key(id)
            || self.parts.contains_key(id)
            || self.transforms.contains_key(id)
            || self.meshes.contains_key(id)
            || self.parameters.contains_key(id)
            || self.bindings.contains_key(id)
            || self.scene_bindings.contains_key(id)
            || self.blend_key_tables.contains_key(id)
            || self.blend_constraints.contains_key(id)
            || self.blend_bindings.contains_key(id)
            || self.glues.contains_key(id)
    }

    fn failed(&self, status: Status) -> EditResult {
        EditResult {
            status,
            changes: ChangeSet {
                kind: ChangeKind::None,
                mesh_ids: Vec::new(),
                revision: self.revision,
                object_ids: Vec::new(),
            },
            referrers: Vec::new(),
        }
    }

    fn changed(
        &mut self,
        kind: ChangeKind,
        mesh_ids: Vec<String>,
        mut object_ids: Vec<String>,
    ) -> EditResult {
        if object_ids.is_empty() {
            object_ids = mesh_ids.clone();
        }
        self.revision += 1;
        EditResult {
            status: Status::ok(),
            changes: ChangeSet {
                kind,
                mesh_ids,
                revision: self.revision,
                object_ids,
            },
            referrers: Vec::new(),
        }
    }

    // --- Assets ---
    pub fn asset_order(&self) -> &[String] {
        &self.asset_order
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    pub fn get_asset(&self, id: &str) -> Option<&ImageAsset> {
        self.assets.get(id)
    }

    pub fn add_asset(&mut self, asset: ImageAsset) -> EditResult {
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
        if !valid_uuid(&asset.id) {
            return self.failed(Status::error(
                "INVALID_ID",
                "Asset ID must be a canonical UUID.",
            ));
        }
        if self.contains_id(&asset.id) {
            return self.failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
        }
        if asset.width == 0 || asset.height == 0 || asset.source.is_empty() {
            return self.failed(Status::error(
                "INVALID_ASSET",
                "Asset requires source and positive pixel dimensions.",
            ));
        }
        let key = asset.id.clone();
        self.assets.insert(key.clone(), asset);
        self.asset_order.push(key.clone());
        self.changed(ChangeKind::Metadata, Vec::new(), vec![key])
    }

    pub fn replace_asset(&mut self, asset: ImageAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &asset.id));
        }
        if !self.assets.contains_key(&asset.id) {
            return self.failed(Status::error("MISSING_ASSET", &asset.id));
        }
        if asset.width == 0 || asset.height == 0 || asset.source.is_empty() {
            return self.failed(Status::error("INVALID_ASSET", &asset.id));
        }
        let key = asset.id.clone();
        self.assets.insert(key.clone(), asset);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Metadata, meshes, vec![key])
    }

    // --- Parts ---
    pub fn part_order(&self) -> &[String] {
        &self.part_order
    }

    pub fn get_part(&self, id: &str) -> Option<&Part> {
        self.parts.get(id)
    }

    fn validate_part(&self, p: &Part) -> Status {
        if !valid_uuid(&p.id) || p.runtime_id.is_empty() {
            return Status::error("INVALID_ID", &p.id);
        }
        for (id, other) in &self.parts {
            if id != &p.id && other.runtime_id == p.runtime_id {
                return Status::error("DUPLICATE_RUNTIME_ID", &p.id);
            }
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

    // --- Transforms ---
    pub fn transform_order(&self) -> &[String] {
        &self.transform_order
    }

    pub fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.transforms.get(id)
    }

    fn validate_transform(&self, t: &Transform) -> Status {
        if !valid_uuid(&t.id) || t.runtime_id.is_empty() {
            return Status::error("INVALID_ID", &t.id);
        }
        for (id, other) in &self.transforms {
            if id != &t.id && other.runtime_id == t.runtime_id {
                return Status::error("DUPLICATE_RUNTIME_ID", &t.id);
            }
        }
        if !t.part_id.is_empty() && !self.parts.contains_key(&t.part_id) {
            return Status::error("MISSING_PART", format!("{}.part_id", t.id));
        }
        let mut seen = HashSet::new();
        seen.insert(t.id.clone());
        let mut parent_id = t.parent_id.clone();
        while !parent_id.is_empty() {
            if !seen.insert(parent_id.clone()) {
                return Status::error("RELATION_CYCLE", format!("{}.parent_id", t.id));
            }
            match self.get_transform(&parent_id) {
                Some(parent) => parent_id = parent.parent_id.clone(),
                None => return Status::error("MISSING_TRANSFORM", parent_id),
            }
        }
        if !t.base_angle.is_finite() {
            return Status::error("NON_FINITE", format!("{}.base_angle", t.id));
        }
        let s = pose_valid(&t.rotation, &t.id);
        if !s.is_ok() {
            return s;
        }
        if t.kind == TransformKind::Warp {
            if t.rows == 0
                || t.columns == 0
                || t.rows > 1024
                || t.columns > 1024
                || t.points.len() != ((t.rows + 1) * (t.columns + 1)) as usize
            {
                return Status::error("INVALID_WARP_GRID", &t.id);
            }
            let s = validate_positions(&t.points);
            if !s.is_ok() {
                return s;
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
        if self.binding_for_scene(&t.id).is_some()
            && (old.kind != t.kind || old.rows != t.rows || old.columns != t.columns)
        {
            return self.failed(Status::error(
                "KEYFORMS_REQUIRED",
                "Remove the scene binding before changing transform grid/type",
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
                visit(&t.parent_id, transforms, seen, result);
                result.push(id.to_string());
            }
        }

        for id in &self.transform_order {
            visit(id, &self.transforms, &mut seen, &mut result);
        }
        result
    }

    // --- Meshes ---
    pub fn mesh_order(&self) -> &[String] {
        &self.mesh_order
    }

    pub fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.meshes.get(id)
    }

    fn validate_mesh_properties(&self, m: &Mesh) -> Status {
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
                                format!("{}: vertex {} used in glue {} is missing from updated mesh", mesh.id, pair.vertex_a, g.id),
                            ));
                        }
                    }
                }
                if g.mesh_b_id == mesh.id {
                    for pair in &g.pairs {
                        if !new_vids.contains(&pair.vertex_b) {
                            return self.failed(Status::error(
                                "GLUE_CONFLICT",
                                format!("{}: vertex {} used in glue {} is missing from updated mesh", mesh.id, pair.vertex_b, g.id),
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
            return self.failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel first"));
        }
        let Some(old_binding) = self.binding_for_mesh(&mesh.id) else {
            return self.failed(Status::error("MISSING_BINDING", &mesh.id));
        };
        if binding.id != old_binding.id || binding.mesh_id != mesh.id {
            return self.failed(Status::error("INVALID_BINDING", "Preserve the binding and mesh IDs"));
        }
        let mesh_id = mesh.id.clone();
        let binding_id = binding.id.clone();
        let mut candidate = self.clone();
        let removed = candidate.erase_object(&binding_id);
        if !removed.status.is_ok() { return self.failed(removed.status); }
        let replaced = candidate.replace_mesh(mesh);
        if !replaced.status.is_ok() { return self.failed(replaced.status); }
        let rebound = candidate.create_binding(binding);
        if !rebound.status.is_ok() { return self.failed(rebound.status); }
        self.meshes = candidate.meshes;
        self.vertex_slots = candidate.vertex_slots;
        self.bindings = candidate.bindings;
        self.changed(ChangeKind::Structure, vec![mesh_id.clone()], vec![mesh_id, binding_id])
    }

    pub fn render_indices(&self, id: &str) -> Result<Vec<u32>, Status> {
        let mesh = self
            .get_mesh(id)
            .ok_or_else(|| Status::error("MISSING_MESH", "Mesh does not exist."))?;
        let slots = self
            .vertex_slots
            .get(id)
            .ok_or_else(|| Status::error("MISSING_MESH", "Vertex slots missing."))?;
        let mut out = Vec::with_capacity(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for vertex in triangle {
                out.push(*slots.get(vertex).unwrap() as u32);
            }
        }
        Ok(out)
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
        mesh.name = name;
        self.changed(ChangeKind::Metadata, vec![id.to_string()], Vec::new())
    }

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

        for delta in deltas {
            let mesh = self.meshes.get_mut(&delta.mesh_id).unwrap();
            for (slot, after) in delta.slots.into_iter().zip(delta.after) {
                mesh.base_positions[slot] = after;
            }
        }

        self.changed(ChangeKind::Positions, changed_meshes, Vec::new())
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

    pub fn begin_transaction(&mut self) -> Status {
        if !self.initialized() {
            return Status::error("NOT_INITIALIZED", "Initialize Document first.");
        }
        if self.transaction_active {
            return Status::error("TRANSACTION_ACTIVE", "A transaction is already active.");
        }
        self.transaction_active = true;
        self.staged_updates.clear();
        Status::ok()
    }

    pub fn stage_vertex_positions(&mut self, update: VertexPositionUpdate) -> Status {
        if !self.transaction_active {
            return Status::error("NO_TRANSACTION", "Call begin_transaction first.");
        }
        self.staged_updates.push(update);
        Status::ok()
    }

    pub fn commit_transaction(&mut self) -> EditResult {
        if !self.transaction_active {
            return self.failed(Status::error("NO_TRANSACTION", "No transaction is active."));
        }
        self.transaction_active = false;
        let staged = std::mem::take(&mut self.staged_updates);
        self.apply_vertex_position_updates(&staged)
    }

    pub fn cancel_transaction(&mut self) -> Status {
        if !self.transaction_active {
            return Status::error("NO_TRANSACTION", "No transaction is active.");
        }
        self.transaction_active = false;
        self.staged_updates.clear();
        Status::ok()
    }

    // --- Parameters ---
    pub fn parameter_order(&self) -> &[String] {
        &self.parameter_order
    }

    pub fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.parameters.get(id)
    }

    fn validate_parameter(&self, p: &Parameter) -> Status {
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
        meshes.sort();
        meshes.dedup();
        let id = p.id.clone();
        self.parameters.insert(id.clone(), p);
        self.changed(ChangeKind::Structure, meshes, vec![id])
    }

    // --- Mesh Bindings ---
    pub fn binding_order(&self) -> &[String] {
        &self.binding_order
    }

    pub fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.bindings.get(id)
    }

    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<&MeshBinding> {
        for key in &self.binding_order {
            if self.bindings[key].mesh_id == mesh_id {
                return Some(&self.bindings[key]);
            }
        }
        None
    }

    fn canonicalize_binding(&self, b: &mut MeshBinding) -> Status {
        if !valid_uuid(&b.id) {
            return Status::error(
                "INVALID_ID",
                format!("{}: binding requires a canonical UUID", b.id),
            );
        }
        let mesh = match self.get_mesh(&b.mesh_id) {
            Some(m) => m,
            None => {
                return Status::error("MISSING_MESH", format!("{}.mesh_id: {}", b.id, b.mesh_id))
            }
        };
        if let Some(old) = self.binding_for_mesh(&b.mesh_id) {
            if old.id != b.id {
                return Status::error(
                    "BINDING_CONFLICT",
                    format!("{}: mesh positions already bound by {}", b.id, old.id),
                );
            }
        }
        if b.axes.is_empty() || b.axes.len() > 16 {
            return Status::error(
                "INVALID_BINDING",
                format!("{}: bindings support 1..16 axes", b.id),
            );
        }
        let mut total = 1usize;
        let mut seen = HashSet::new();
        for axis in &b.axes {
            let p = match self.get_parameter(&axis.parameter_id) {
                Some(p) => p,
                None => {
                    return Status::error(
                        "MISSING_PARAMETER",
                        format!("{}.axes: {}", b.id, axis.parameter_id),
                    )
                }
            };
            if !seen.insert(p.id.clone()) {
                return Status::error("DUPLICATE_AXIS", format!("{}.axes: {}", b.id, p.id));
            }
            if axis.keys.is_empty() || axis.keys.len() > (i32::MAX as usize) / total {
                return Status::error(
                    "INVALID_KEYS",
                    format!("{}.axes: empty or overflowing key grid", b.id),
                );
            }
            total *= axis.keys.len();
            for (i, &k) in axis.keys.iter().enumerate() {
                if !k.is_finite()
                    || k < p.minimum
                    || k > p.maximum
                    || (i > 0 && k <= axis.keys[i - 1])
                {
                    return Status::error(
                        "INVALID_KEYS",
                        format!(
                            "{}.axes[{}]: finite, strictly increasing keys within parameter range required",
                            b.id, p.id
                        ),
                    );
                }
            }
        }
        if b.keyforms.len() != total {
            return Status::error(
                "INCOMPLETE_KEYFORMS",
                format!("{}: require every Cartesian key combination", b.id),
            );
        }
        let mut ordered = vec![MeshKeyform::default(); total];
        let mut occupied = vec![false; total];
        for form in b.keyforms.drain(..) {
            if form.keys.len() != b.axes.len() || form.positions.len() != mesh.vertex_ids.len() {
                return Status::error(
                    "INVALID_LENGTH",
                    format!(
                        "{}.keyforms: axes and positions must match binding and mesh",
                        b.id
                    ),
                );
            }
            let s = validate_positions(&form.positions);
            if !s.is_ok() {
                return Status::error(s.code, format!("{}.keyforms: {}", b.id, s.message));
            }
            let s = validate_appearance(&form.appearance, &b.id);
            if !s.is_ok() {
                return s;
            }
            if let Some(order) = form.draw_order {
                let s = validate_draw_order(order, &b.id);
                if !s.is_ok() {
                    return s;
                }
            }
            let mut index = 0usize;
            let mut stride = 1usize;
            for (a, axis) in b.axes.iter().enumerate() {
                let key_pos = match axis.keys.iter().position(|&k| k == form.keys[a]) {
                    Some(idx) => idx,
                    None => {
                        return Status::error(
                            "INVALID_KEY_COMBINATION",
                            format!("{}.keyforms: unknown key value", b.id),
                        )
                    }
                };
                index += key_pos * stride;
                stride *= axis.keys.len();
            }
            if occupied[index] {
                return Status::error(
                    "DUPLICATE_KEYFORM",
                    format!("{}.keyforms: repeated combination", b.id),
                );
            }
            occupied[index] = true;
            ordered[index] = form;
        }
        b.keyforms = ordered;
        Status::ok()
    }

    pub fn create_binding(&mut self, mut b: MeshBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let mesh = b.mesh_id.clone();
        self.bindings.insert(id.clone(), b);
        self.binding_order.push(id.clone());
        self.changed(ChangeKind::Structure, vec![mesh], vec![id])
    }

    pub fn replace_binding(&mut self, mut b: MeshBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let previous_mesh = match self.bindings.get(&b.id) {
            Some(old) => old.mesh_id.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", &b.id)),
        };
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let mesh = b.mesh_id.clone();
        let mut affected = vec![mesh.clone()];
        if previous_mesh != mesh {
            affected.push(previous_mesh.clone());
        }
        self.bindings.insert(id.clone(), b);
        self.changed(
            ChangeKind::Structure,
            affected,
            vec![id, mesh, previous_mesh],
        )
    }

    pub fn set_mesh_keyform(&mut self, id: &str, form: MeshKeyform) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let mut b = match self.get_binding(id) {
            Some(old) => old.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", id)),
        };
        let it = b.keyforms.iter_mut().find(|f| f.keys == form.keys);
        match it {
            Some(f) => *f = form,
            None => return self.failed(Status::error("INVALID_KEY_COMBINATION", id)),
        }
        let s = self.canonicalize_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let mesh = b.mesh_id.clone();
        self.bindings.insert(id.to_string(), b);
        self.changed(
            ChangeKind::Positions,
            vec![mesh.clone()],
            vec![id.to_string(), mesh],
        )
    }

    // --- Scene Bindings ---
    pub fn scene_binding_order(&self) -> &[String] {
        &self.scene_binding_order
    }

    pub fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.scene_bindings.get(id)
    }

    pub fn binding_for_scene(&self, target_id: &str) -> Option<&SceneBinding> {
        for key in &self.scene_binding_order {
            if self.scene_bindings[key].target_id == target_id {
                return Some(&self.scene_bindings[key]);
            }
        }
        None
    }

    fn canonicalize_scene_binding(&self, b: &mut SceneBinding) -> Status {
        if !valid_uuid(&b.id) {
            return Status::error("INVALID_ID", &b.id);
        }
        let t = self.get_transform(&b.target_id);
        let p = self.get_part(&b.target_id);
        if t.is_none() && p.is_none() {
            return Status::error("MISSING_OBJECT", &b.target_id);
        }
        if let Some(old) = self.binding_for_scene(&b.target_id) {
            if old.id != b.id {
                return Status::error("BINDING_CONFLICT", &b.target_id);
            }
        }
        if b.axes.is_empty() || b.axes.len() > 16 {
            return Status::error("INVALID_BINDING", &b.id);
        }
        let mut total = 1usize;
        let mut seen = HashSet::new();
        for a in &b.axes {
            let param = match self.get_parameter(&a.parameter_id) {
                Some(param) => param,
                None => return Status::error("MISSING_PARAMETER", &a.parameter_id),
            };
            if !seen.insert(a.parameter_id.clone()) {
                return Status::error("DUPLICATE_AXIS", &b.id);
            }
            if a.keys.is_empty() || a.keys.len() > (i32::MAX as usize) / total {
                return Status::error("INVALID_KEYS", &b.id);
            }
            total *= a.keys.len();
            for (i, &k) in a.keys.iter().enumerate() {
                if !k.is_finite()
                    || k < param.minimum
                    || k > param.maximum
                    || (i > 0 && k <= a.keys[i - 1])
                {
                    return Status::error("INVALID_KEYS", &b.id);
                }
            }
        }
        if total != b.keyforms.len() {
            return Status::error("INCOMPLETE_KEYFORMS", &b.id);
        }
        let mut ordered = vec![SceneKeyform::default(); total];
        let mut occupied = vec![false; total];
        for f in b.keyforms.drain(..) {
            let expected_pos_len = if let Some(tr) = t {
                if tr.kind == TransformKind::Warp {
                    tr.points.len()
                } else {
                    0
                }
            } else {
                0
            };
            if f.keys.len() != b.axes.len() || f.positions.len() != expected_pos_len {
                return Status::error("INVALID_LENGTH", &b.id);
            }
            let s = validate_positions(&f.positions);
            if !s.is_ok() {
                return s;
            }
            let s = pose_valid(&f.rotation, &b.id);
            if !s.is_ok() {
                return s;
            }
            let s = validate_appearance(&f.appearance, &b.id);
            if !s.is_ok() {
                return s;
            }
            let s = validate_draw_order(f.draw_order, &b.id);
            if !s.is_ok() {
                return s;
            }
            let mut index = 0usize;
            let mut stride = 1usize;
            for (a, axis) in b.axes.iter().enumerate() {
                let key_pos = match axis.keys.iter().position(|&k| k == f.keys[a]) {
                    Some(pos) => pos,
                    None => return Status::error("INVALID_KEY_COMBINATION", &b.id),
                };
                index += key_pos * stride;
                stride *= axis.keys.len();
            }
            if occupied[index] {
                return Status::error("DUPLICATE_KEYFORM", &b.id);
            }
            occupied[index] = true;
            ordered[index] = f;
        }
        b.keyforms = ordered;
        Status::ok()
    }

    pub fn create_scene_binding(&mut self, mut b: SceneBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.canonicalize_scene_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        self.scene_bindings.insert(id.clone(), b);
        self.scene_binding_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target])
    }

    pub fn replace_scene_binding(&mut self, mut b: SceneBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        let previous = match self.scene_bindings.get(&b.id) {
            Some(old) => old.target_id.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", &b.id)),
        };
        let s = self.canonicalize_scene_binding(&mut b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        self.scene_bindings.insert(id.clone(), b);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, previous])
    }

    pub fn set_scene_keyform(&mut self, id: &str, f: SceneKeyform) -> EditResult {
        let mut b = match self.get_scene_binding(id) {
            Some(old) => old.clone(),
            None => return self.failed(Status::error("MISSING_BINDING", id)),
        };
        let it = b.keyforms.iter_mut().find(|v| v.keys == f.keys);
        match it {
            Some(entry) => *entry = f,
            None => return self.failed(Status::error("INVALID_KEY_COMBINATION", id)),
        }
        self.replace_scene_binding(b)
    }

    pub fn replace_canvas(&mut self, canvas: Canvas) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &self.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &self.id));
        }
        let mut candidate = Document::new();
        let s = candidate.initialize(self.id.clone(), canvas);
        if !s.is_ok() {
            return self.failed(s);
        }
        self.canvas = canvas;
        let meshes = self.mesh_order.clone();
        let doc_id = self.id.clone();
        self.changed(ChangeKind::Structure, meshes, vec![doc_id])
    }

    // --- References & Erase ---
    pub fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.draw_order_groups.as_deref()
    }

    pub fn replace_draw_order_groups(&mut self, groups: Vec<DrawOrderGroup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let groups = match validate_groups(self, &groups) {
            Ok(groups) => groups,
            Err(status) => return self.failed(status),
        };
        self.draw_order_groups = Some(groups);
        self.changed(ChangeKind::Structure, self.mesh_order.clone(), Vec::new())
    }

    // --- BlendShape Key Tables ---
    pub fn blend_key_table_order(&self) -> &[String] {
        &self.blend_key_table_order
    }

    pub fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.blend_key_tables.get(id)
    }

    fn validate_blend_key_table(&self, table: &BlendShapeKeyTable) -> Status {
        if !valid_uuid(&table.id) {
            return Status::error("INVALID_ID", &table.id);
        }
        let param = match self.get_parameter(&table.parameter_id) {
            Some(p) => p,
            None => return Status::error("MISSING_PARAMETER", &table.parameter_id),
        };
        if param.kind != ParameterKind::BlendShape {
            return Status::error(
                "INVALID_PARAMETER_KIND",
                format!("{}: expected blend_shape parameter", table.parameter_id),
            );
        }
        if table.keys.is_empty() {
            return Status::error("INVALID_KEYS", "Key table must contain at least one key");
        }
        if table.base_key_idx >= table.keys.len() {
            return Status::error("INVALID_BASE_KEY", "Base key index out of bounds");
        }
        for (i, &k) in table.keys.iter().enumerate() {
            if !k.is_finite()
                || k < param.minimum
                || k > param.maximum
                || (i > 0 && k <= table.keys[i - 1])
            {
                return Status::error(
                    "INVALID_KEYS",
                    "Keys must be finite, strictly increasing, and within parameter range",
                );
            }
        }
        Status::ok()
    }

    pub fn create_blend_key_table(&mut self, table: BlendShapeKeyTable) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &table.id));
        }
        if self.contains_id(&table.id) {
            return self.failed(Status::error("DUPLICATE_ID", &table.id));
        }
        let s = self.validate_blend_key_table(&table);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = table.id.clone();
        let param_id = table.parameter_id.clone();
        self.blend_key_tables.insert(id.clone(), table);
        self.blend_key_table_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn replace_blend_key_table(&mut self, table: BlendShapeKeyTable) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &table.id));
        }
        if !self.blend_key_tables.contains_key(&table.id) {
            return self.failed(Status::error("MISSING_KEY_TABLE", &table.id));
        }
        let s = self.validate_blend_key_table(&table);
        if !s.is_ok() {
            return self.failed(s);
        }
        for b in self.blend_bindings.values() {
            if b.key_table_id == table.id && b.keyforms.len() != table.keys.len() {
                return self.failed(Status::error(
                    "KEYFORMS_REQUIRED",
                    format!(
                        "{}: binding keyform count does not match new key table",
                        b.id
                    ),
                ));
            }
        }
        let id = table.id.clone();
        let param_id = table.parameter_id.clone();
        self.blend_key_tables.insert(id.clone(), table);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    // --- BlendShape Constraints ---
    pub fn blend_constraint_order(&self) -> &[String] {
        &self.blend_constraint_order
    }

    pub fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.blend_constraints.get(id)
    }

    fn validate_blend_constraint(&self, constraint: &BlendShapeConstraint) -> Status {
        if !valid_uuid(&constraint.id) {
            return Status::error("INVALID_ID", &constraint.id);
        }
        let param = match self.get_parameter(&constraint.parameter_id) {
            Some(p) => p,
            None => return Status::error("MISSING_PARAMETER", &constraint.parameter_id),
        };
        if param.kind != ParameterKind::BlendShape {
            return Status::error(
                "INVALID_PARAMETER_KIND",
                format!("{}: expected blend_shape parameter", constraint.parameter_id),
            );
        }
        if constraint.keys.len() != constraint.weights.len() || constraint.keys.is_empty() {
            return Status::error(
                "INVALID_LENGTH",
                "Constraint keys and weights must have matching non-zero lengths",
            );
        }
        for (i, (&k, &w)) in constraint.keys.iter().zip(constraint.weights.iter()).enumerate() {
            if !k.is_finite()
                || k < param.minimum
                || k > param.maximum
                || (i > 0 && k <= constraint.keys[i - 1])
            {
                return Status::error(
                    "INVALID_KEYS",
                    "Constraint keys must be finite, strictly increasing, and within parameter range",
                );
            }
            if !w.is_finite() || w < 0.0 {
                return Status::error(
                    "INVALID_WEIGHT",
                    "Constraint weights must be finite and non-negative",
                );
            }
        }
        Status::ok()
    }

    pub fn create_blend_constraint(&mut self, constraint: BlendShapeConstraint) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &constraint.id));
        }
        if self.contains_id(&constraint.id) {
            return self.failed(Status::error("DUPLICATE_ID", &constraint.id));
        }
        let s = self.validate_blend_constraint(&constraint);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = constraint.id.clone();
        let param_id = constraint.parameter_id.clone();
        self.blend_constraints.insert(id.clone(), constraint);
        self.blend_constraint_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn replace_blend_constraint(&mut self, constraint: BlendShapeConstraint) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &constraint.id));
        }
        if !self.blend_constraints.contains_key(&constraint.id) {
            return self.failed(Status::error("MISSING_CONSTRAINT", &constraint.id));
        }
        let s = self.validate_blend_constraint(&constraint);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = constraint.id.clone();
        let param_id = constraint.parameter_id.clone();
        self.blend_constraints.insert(id.clone(), constraint);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    // --- BlendShape Bindings ---
    pub fn blend_binding_order(&self) -> &[String] {
        &self.blend_binding_order
    }

    pub fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.blend_bindings.get(id)
    }

    pub fn blend_bindings_for_target(&self, target_id: &str) -> Vec<&BlendShapeBinding> {
        self.blend_binding_order
            .iter()
            .filter_map(|id| self.blend_bindings.get(id))
            .filter(|b| b.target_id == target_id)
            .collect()
    }

    fn validate_blend_binding(&self, b: &BlendShapeBinding) -> Status {
        if !valid_uuid(&b.id) {
            return Status::error("INVALID_ID", &b.id);
        }
        let key_table = match self.get_blend_key_table(&b.key_table_id) {
            Some(kt) => kt,
            None => return Status::error("MISSING_KEY_TABLE", &b.key_table_id),
        };
        for c_id in &b.constraint_ids {
            if !self.blend_constraints.contains_key(c_id) {
                return Status::error("MISSING_CONSTRAINT", c_id);
            }
        }
        if b.keyforms.len() != key_table.keys.len() {
            return Status::error(
                "INCOMPLETE_KEYFORMS",
                format!(
                    "{}: keyforms length {} does not match key table {}",
                    b.id,
                    b.keyforms.len(),
                    key_table.keys.len()
                ),
            );
        }
        match (&b.target_kind, &b.keyforms) {
            (BlendShapeTargetKind::Mesh, DeltaKeyforms::Mesh(forms)) => {
                let mesh = match self.get_mesh(&b.target_id) {
                    Some(m) => m,
                    None => return Status::error("MISSING_MESH", &b.target_id),
                };
                for f in forms {
                    if f.positions.len() != mesh.vertex_ids.len() {
                        return Status::error(
                            "INVALID_LENGTH",
                            format!(
                                "{}: delta positions len {} != mesh vertex len {}",
                                b.id,
                                f.positions.len(),
                                mesh.vertex_ids.len()
                            ),
                        );
                    }
                    for p in &f.positions {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            return Status::error(
                                "INVALID_POSITION",
                                format!("{}: non-finite delta position", b.id),
                            );
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(order) = f.draw_order {
                        let s = validate_draw_order(order, &b.id);
                        if !s.is_ok() {
                            return s;
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Warp, DeltaKeyforms::Warp(forms)) => {
                let warp = match self.get_transform(&b.target_id) {
                    Some(w) if w.kind == TransformKind::Warp => w,
                    _ => {
                        return Status::error(
                            "MISSING_OBJECT",
                            format!("{}: expected Warp transform", b.target_id),
                        )
                    }
                };
                for f in forms {
                    if f.points.len() != warp.points.len() {
                        return Status::error(
                            "INVALID_LENGTH",
                            format!(
                                "{}: delta points len {} != warp points len {}",
                                b.id,
                                f.points.len(),
                                warp.points.len()
                            ),
                        );
                    }
                    for p in &f.points {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            return Status::error(
                                "INVALID_POSITION",
                                format!("{}: non-finite delta point", b.id),
                            );
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Rotation, DeltaKeyforms::Rotation(forms)) => {
                match self.get_transform(&b.target_id) {
                    Some(r) if r.kind == TransformKind::Rotation => r,
                    _ => {
                        return Status::error(
                            "MISSING_OBJECT",
                            format!("{}: expected Rotation transform", b.target_id),
                        )
                    }
                };
                for f in forms {
                    if let Some(orig) = f.origin {
                        if !orig.x.is_finite() || !orig.y.is_finite() {
                            return Status::error("INVALID_POSITION", &b.id);
                        }
                    }
                    if let Some(angle) = f.angle {
                        if !angle.is_finite() {
                            return Status::error("INVALID_ROTATION", &b.id);
                        }
                    }
                    if let Some(scale) = f.scale {
                        if !scale.is_finite() || scale < 0.0 {
                            return Status::error("INVALID_ROTATION", &b.id);
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Part, DeltaKeyforms::Part(forms)) => {
                if !self.parts.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Part", b.target_id),
                    );
                }
                for f in forms {
                    let s = validate_draw_order(f.draw_order, &b.id);
                    if !s.is_ok() {
                        return s;
                    }
                }
            }
            _ => {
                return Status::error(
                    "MISMATCHED_TARGET_KIND",
                    format!(
                        "{}: target_kind {:?} does not match keyform type",
                        b.id, b.target_kind
                    ),
                );
            }
        }
        Status::ok()
    }

    pub fn create_blend_binding(&mut self, b: BlendShapeBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.validate_blend_binding(&b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        let key_table = b.key_table_id.clone();
        self.blend_bindings.insert(id.clone(), b);
        self.blend_binding_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, key_table])
    }

    pub fn replace_blend_binding(&mut self, b: BlendShapeBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if !self.blend_bindings.contains_key(&b.id) {
            return self.failed(Status::error("MISSING_BINDING", &b.id));
        }
        let s = self.validate_blend_binding(&b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        let key_table = b.key_table_id.clone();
        self.blend_bindings.insert(id.clone(), b);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, key_table])
    }

    // --- Glues ---
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

    fn validate_glue(&self, glue: &Glue) -> Status {
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
        if !glue.intensity.is_finite() || glue.intensity < 0.0 {
            return Status::error(
                "INVALID_INTENSITY",
                format!("{}: intensity must be finite and non-negative", glue.id),
            );
        }
        if let Some(b_id) = &glue.binding_id {
            if !self.bindings.contains_key(b_id) {
                return Status::error("MISSING_BINDING", b_id);
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
            if t.parent_id == id || t.part_id == id {
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
            let mut refers = b.target_id == id;
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
            if g.mesh_a_id == id || g.mesh_b_id == id || g.binding_id.as_deref() == Some(id) {
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
        if let Some(b) = self.get_blend_binding(id) {
            if b.target_kind == BlendShapeTargetKind::Mesh {
                meshes.push(b.target_id.clone());
            }
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
        self.meshes.remove(id);
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

        meshes.sort();
        meshes.dedup();
        self.changed(ChangeKind::Structure, meshes, vec![id.to_string()])
    }
}

trait EmptyOr {
    fn empty_or(&self) -> bool;
}

impl<T> EmptyOr for Vec<T> {
    fn empty_or(&self) -> bool {
        self.is_empty()
    }
}
