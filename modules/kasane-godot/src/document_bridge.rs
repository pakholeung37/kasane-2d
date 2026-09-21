use godot::prelude::*;
use std::collections::HashMap;

use kasane_core::document::Document;
use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{
    BlendMode, Canvas, ChangeKind, EditResult, ImageAsset, Mesh, RotationPose, Status, Transform,
    TransformKind, Vec2, VertexPositionUpdate,
};
use kasane_project::store::DocumentSession;

use crate::conversions::{
    binding_from_dict, dict_from_binding, dict_from_mesh_properties, dict_from_parameter,
    dict_from_part, dict_from_scene_binding, dict_from_transform, error_dict, ids_to_packed,
    is_main_thread, mesh_properties_from_dict, packed_to_ids, packed_to_vectors,
    parameter_from_dict, part_from_dict, scene_binding_from_dict, status_to_dict,
    transform_from_dict, vectors_to_packed, Array, Dictionary,
};
use crate::deformer_data::KasaneDeformerData;
use crate::mesh_data::KasaneMeshData;

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct KasaneDocumentState {
    base: Base<RefCounted>,
    pub document: Document,
    pub owner_id: u64,
    pub generation: u64,
}

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct KasaneDocumentBridge {
    base: Base<RefCounted>,
    session: DocumentSession,
    generation: u64,
    preview_values: HashMap<String, f32>,
    object_epochs: HashMap<String, u64>,
}

#[godot_api]
impl IRefCounted for KasaneDocumentBridge {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            session: DocumentSession::new(),
            generation: 1,
            preview_values: HashMap::new(),
            object_epochs: HashMap::new(),
        }
    }
}

#[godot_api]
impl KasaneDocumentBridge {
    #[signal]
    fn changed(change: Dictionary);

    #[signal]
    fn preview_changed();

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn increment_generation(&mut self) {
        self.generation += 1;
        self.object_epochs.clear();
    }

    pub fn object_epoch(&self, id: &str) -> u64 {
        *self.object_epochs.get(id).unwrap_or(&0)
    }

    pub fn session(&self) -> &DocumentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut DocumentSession {
        &mut self.session
    }

    pub fn preview_values_mut(&mut self) -> &mut HashMap<String, f32> {
        &mut self.preview_values
    }

    pub fn evaluate(&self, out: &mut DrawableFrame) -> Status {
        evaluate_frame(self.session.document(), &self.preview_values, out)
    }

    pub fn apply(&mut self, edit: EditResult) -> Dictionary {
        let mut out = status_to_dict(&edit.status);
        out.set("revision", edit.changes.revision as i64);
        let kind_str = match edit.changes.kind {
            ChangeKind::Structure => "structure",
            ChangeKind::Positions => "positions",
            ChangeKind::Metadata => "metadata",
            ChangeKind::None => "none",
        };
        out.set("change_kind", kind_str);
        let mut changed_meshes = PackedStringArray::new();
        for id in &edit.changes.mesh_ids {
            changed_meshes.push(&GString::from(id.as_str()));
        }
        out.set("changed_meshes", &changed_meshes);
        let mut objects = PackedStringArray::new();
        for id in &edit.changes.object_ids {
            objects.push(&GString::from(id.as_str()));
        }
        out.set("changed_objects", &objects);
        let mut referrers = PackedStringArray::new();
        for id in &edit.referrers {
            referrers.push(&GString::from(id.as_str()));
        }
        out.set("referrers", &referrers);

        if !edit.status.is_ok() {
            return out;
        }

        self.preview_values
            .retain(|k, _| self.session.document().get_parameter(k).is_some());

        if edit.changes.kind != ChangeKind::None {
            self.base_mut().emit_signal("changed", &[out.to_variant()]);
        }
        out
    }

    #[func]
    pub fn initialize(
        &mut self,
        id: GString,
        canvas_size: Vector2,
        #[opt(default = Vector2::ZERO)] origin: Vector2,
        #[opt(default = 1.0)] pixels_per_unit: f64,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let canvas = Canvas::new(
            canvas_size.x,
            canvas_size.y,
            Vec2::new(origin.x, origin.y),
            pixels_per_unit as f32,
        );
        let status = self
            .session
            .document_mut()
            .initialize(id.to_string(), canvas);
        status_to_dict(&status)
    }

    /// Validate a replacement before discarding the active project and its path.
    #[func]
    pub fn new_project(
        &mut self,
        id: GString,
        canvas_size: Vector2,
        #[opt(default = Vector2::ZERO)] origin: Vector2,
        #[opt(default = 1.0)] pixels_per_unit: f64,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let mut next = DocumentSession::new();
        let status = next.document_mut().initialize(
            id.to_string(),
            Canvas::new(
                canvas_size.x,
                canvas_size.y,
                Vec2::new(origin.x, origin.y),
                pixels_per_unit as f32,
            ),
        );
        if !status.is_ok() {
            return status_to_dict(&status);
        }
        self.session = next;
        self.increment_generation();
        self.preview_values.clear();
        let mut out = status_to_dict(&status);
        out.set("generation", self.generation as i64);
        out.set("revision", self.session.document().revision() as i64);
        out.set("change_kind", "structure");
        self.base_mut().emit_signal("changed", &[out.to_variant()]);
        out
    }

    #[func]
    pub fn add_image_asset(
        &mut self,
        id: GString,
        name: GString,
        source: GString,
        width: i64,
        height: i64,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        if width <= 0 || height <= 0 || width > u32::MAX as i64 || height > u32::MAX as i64 {
            return error_dict(
                "INVALID_ASSET",
                "Dimensions must be positive uint32 values.",
            );
        }
        let asset = ImageAsset {
            id: id.to_string(),
            name: name.to_string(),
            source: source.to_string(),
            width: width as u32,
            height: height as u32,
            sha256: String::new(),
        };
        let edit = self.session.document_mut().add_asset(asset);
        self.apply(edit)
    }

    fn write_mesh_internal(
        &mut self,
        d: Dictionary,
        replace: bool,
        binding: Option<Dictionary>,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let required = [
            "id",
            "name",
            "texture_asset_id",
            "vertex_ids",
            "base_positions",
            "uvs",
            "triangles",
        ];
        for &key in &required {
            if !d.contains_key(key) {
                return error_dict(
                    "INVALID_FIELD",
                    "Missing field or incorrect field type; see Document specification.",
                );
            }
        }
        for (key, expected) in [
            ("vertex_ids", VariantType::PACKED_INT64_ARRAY),
            ("base_positions", VariantType::PACKED_VECTOR2_ARRAY),
            ("uvs", VariantType::PACKED_VECTOR2_ARRAY),
            ("triangles", VariantType::PACKED_INT64_ARRAY),
        ] {
            if d.get(key).unwrap().get_type() != expected {
                return error_dict(
                    "INVALID_FIELD",
                    &format!("{key} requires its declared Packed array type"),
                );
            }
        }
        let Ok(id) = d.get("id").unwrap().try_to::<GString>() else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };
        let Ok(name) = d.get("name").unwrap().try_to::<GString>() else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };
        let Ok(texture_asset_id) = d.get("texture_asset_id").unwrap().try_to::<GString>() else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };
        let Ok(vertex_ids_raw) = d.get("vertex_ids").unwrap().try_to::<PackedInt64Array>() else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };
        let Ok(base_pos_raw) = d
            .get("base_positions")
            .unwrap()
            .try_to::<PackedVector2Array>()
        else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };
        let Ok(uvs_raw) = d.get("uvs").unwrap().try_to::<PackedVector2Array>() else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };
        let Ok(triangles_raw) = d.get("triangles").unwrap().try_to::<PackedInt64Array>() else {
            return error_dict("INVALID_FIELD", "Missing field or incorrect field type");
        };

        let mut mesh = Mesh {
            id: id.to_string(),
            name: name.to_string(),
            texture_asset_id: texture_asset_id.to_string(),
            ..Default::default()
        };

        if d.contains_key("runtime_id") {
            let Ok(r_id) = d.get("runtime_id").unwrap().try_to::<GString>() else {
                return error_dict("INVALID_FIELD", "runtime_id must be a string.");
            };
            mesh.runtime_id = r_id.to_string();
        } else if replace {
            if let Some(old) = self.session.document().get_mesh(&mesh.id) {
                mesh.runtime_id = old.runtime_id.clone();
            }
        }

        if d.contains_key("properties") {
            let Ok(props) = d.get("properties").unwrap().try_to::<Dictionary>() else {
                return error_dict("INVALID_FIELD", "properties must be a dictionary");
            };
            if let Err(s) = mesh_properties_from_dict(&props, &mut mesh) {
                return status_to_dict(&s);
            }
        } else if replace {
            if let Some(old) = self.session.document().get_mesh(&mesh.id) {
                mesh.part_id = old.part_id.clone();
                mesh.deformer_id = old.deformer_id.clone();
                mesh.blend_mode = old.blend_mode;
                mesh.raw_blend_mode = old.raw_blend_mode;
                mesh.enabled = old.enabled;
                mesh.double_sided = old.double_sided;
                mesh.inverted_mask = old.inverted_mask;
                mesh.appearance = old.appearance;
                mesh.draw_order = old.draw_order;
                mesh.masks = old.masks.clone();
            }
        }

        let vertex_ids = match packed_to_ids(&vertex_ids_raw) {
            Ok(v) => v,
            Err(s) => return status_to_dict(&s),
        };
        mesh.vertex_ids = vertex_ids;
        mesh.base_positions = packed_to_vectors(&base_pos_raw);
        mesh.uvs = packed_to_vectors(&uvs_raw);

        let tri_ids = match packed_to_ids(&triangles_raw) {
            Ok(v) => v,
            Err(s) => return status_to_dict(&s),
        };
        if tri_ids.len() % 3 != 0 {
            return error_dict(
                "INVALID_LENGTH",
                "Triangle vertex IDs must be a multiple of three.",
            );
        }
        mesh.triangles = tri_ids.as_chunks::<3>().0.to_vec();

        let edit = if let Some(raw) = binding {
            let binding = match binding_from_dict(&raw) {
                Ok(binding) => binding,
                Err(status) => return status_to_dict(&status),
            };
            self.session
                .document_mut()
                .replace_mesh_with_keyforms(mesh, binding)
        } else if replace {
            self.session.document_mut().replace_mesh(mesh)
        } else {
            self.session.document_mut().create_mesh(mesh)
        };
        self.apply(edit)
    }

    #[func]
    pub fn create_mesh(&mut self, description: Dictionary) -> Dictionary {
        self.write_mesh_internal(description, false, None)
    }

    #[func]
    pub fn replace_mesh(&mut self, description: Dictionary) -> Dictionary {
        self.write_mesh_internal(description, true, None)
    }

    #[func]
    pub fn replace_mesh_with_keyforms(
        &mut self,
        description: Dictionary,
        binding: Dictionary,
    ) -> Dictionary {
        self.write_mesh_internal(description, true, Some(binding))
    }

    #[func]
    pub fn get_mesh(&self, id: GString) -> Option<Gd<KasaneMeshData>> {
        if !is_main_thread() {
            return None;
        }
        self.session.document().get_mesh(&id.to_string())?;
        let mut handle = Gd::<KasaneMeshData>::default();
        handle.bind_mut().attach(
            self.base().instance_id().to_i64() as u64,
            self.generation,
            self.object_epoch(&id.to_string()),
            id,
        );
        Some(handle)
    }

    #[func]
    pub fn create_parameter(&mut self, description: Dictionary) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let p = match parameter_from_dict(&description) {
            Ok(p) => p,
            Err(s) => return status_to_dict(&s),
        };
        let edit = self.session.document_mut().create_parameter(p);
        self.apply(edit)
    }

    #[func]
    pub fn replace_parameter(&mut self, description: Dictionary) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let p = match parameter_from_dict(&description) {
            Ok(p) => p,
            Err(s) => return status_to_dict(&s),
        };
        let edit = self.session.document_mut().replace_parameter(p);
        self.apply(edit)
    }

    #[func]
    pub fn get_parameter(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        if let Some(p) = self.session.document().get_parameter(&id.to_string()) {
            dict_from_parameter(p)
        } else {
            Dictionary::new()
        }
    }

    #[func]
    pub fn references_to(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        if !self.session.document().contains_id(&id.to_string()) {
            return error_dict("MISSING_OBJECT", &id.to_string());
        }
        let mut out = status_to_dict(&Status::ok());
        let refs = PackedStringArray::from_iter(
            self.session
                .document()
                .references_to(&id.to_string())
                .iter()
                .map(GString::from),
        );
        out.set("referrers", &refs);
        out
    }

    #[func]
    pub fn write_binding(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let b = match binding_from_dict(&description) {
            Ok(b) => b,
            Err(s) => return status_to_dict(&s),
        };
        let edit = if replace {
            self.session.document_mut().replace_binding(b)
        } else {
            self.session.document_mut().create_binding(b)
        };
        self.apply(edit)
    }

    #[func]
    pub fn write_blend_key_table(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let value = match crate::conversions::structured_from_dict::<
            kasane_core::types::BlendShapeKeyTable,
        >(&description)
        {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
        let edit = if replace {
            self.session.document_mut().replace_blend_key_table(value)
        } else {
            self.session.document_mut().create_blend_key_table(value)
        };
        self.apply(edit)
    }

    #[func]
    pub fn get_blend_key_table_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        match self.session.document().get_blend_key_table(&id.to_string()) {
            Some(value) => crate::conversions::structured_to_dict(value),
            None => error_dict("MISSING_OBJECT", &id.to_string()),
        }
    }

    #[func]
    pub fn write_blend_constraint(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let value = match crate::conversions::structured_from_dict::<
            kasane_core::types::BlendShapeConstraint,
        >(&description)
        {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
        let edit = if replace {
            self.session.document_mut().replace_blend_constraint(value)
        } else {
            self.session.document_mut().create_blend_constraint(value)
        };
        self.apply(edit)
    }

    #[func]
    pub fn get_blend_constraint_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        match self
            .session
            .document()
            .get_blend_constraint(&id.to_string())
        {
            Some(value) => crate::conversions::structured_to_dict(value),
            None => error_dict("MISSING_OBJECT", &id.to_string()),
        }
    }

    #[func]
    pub fn write_blend_binding(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let value = match crate::conversions::structured_from_dict::<
            kasane_core::types::BlendShapeBinding,
        >(&description)
        {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
        let edit = if replace {
            self.session.document_mut().replace_blend_binding(value)
        } else {
            self.session.document_mut().create_blend_binding(value)
        };
        self.apply(edit)
    }

    #[func]
    pub fn get_blend_binding_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        match self.session.document().get_blend_binding(&id.to_string()) {
            Some(value) => crate::conversions::structured_to_dict(value),
            None => error_dict("MISSING_OBJECT", &id.to_string()),
        }
    }

    #[func]
    pub fn write_glue(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let value = match crate::conversions::structured_from_dict::<kasane_core::types::Glue>(
            &description,
        ) {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
        let edit = if replace {
            self.session.document_mut().replace_glue(value)
        } else {
            self.session.document_mut().create_glue(value)
        };
        self.apply(edit)
    }

    #[func]
    pub fn get_glue_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        match self.session.document().get_glue(&id.to_string()) {
            Some(value) => crate::conversions::structured_to_dict(value),
            None => error_dict("MISSING_OBJECT", &id.to_string()),
        }
    }

    #[func]
    pub fn write_offscreen(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let value = match crate::conversions::structured_from_dict::<kasane_core::types::Offscreen>(
            &description,
        ) {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
        let edit = if replace {
            self.session.document_mut().replace_offscreen(value)
        } else {
            self.session.document_mut().create_offscreen(value)
        };
        self.apply(edit)
    }

    #[func]
    pub fn get_offscreen_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        match self.session.document().get_offscreen(&id.to_string()) {
            Some(value) => crate::conversions::structured_to_dict(value),
            None => error_dict("MISSING_OBJECT", &id.to_string()),
        }
    }

    #[func]
    pub fn replace_mesh_topology(&mut self, description: Dictionary) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        #[derive(serde::Deserialize)]
        struct Replacement {
            mesh: Mesh,
            binding: Option<kasane_core::types::MeshBinding>,
            blend_bindings: Vec<kasane_core::types::BlendShapeBinding>,
            glues: Vec<kasane_core::types::Glue>,
            vertex_mapping: Vec<(u32, Option<u32>)>,
        }
        let value = match crate::conversions::structured_from_dict::<Replacement>(&description) {
            Ok(value) => value,
            Err(status) => return status_to_dict(&status),
        };
        let mapping: HashMap<_, _> = value.vertex_mapping.iter().copied().collect();
        if mapping.len() != value.vertex_mapping.len() {
            return error_dict("INVALID_VERTEX_MAPPING", "Duplicate source vertex");
        }
        let edit = self.session.document_mut().replace_mesh_topology(
            value.mesh,
            value.binding,
            value.blend_bindings,
            value.glues,
            mapping,
        );
        self.apply(edit)
    }

    #[func]
    pub fn get_mesh_topology_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let doc = self.session.document();
        let id = id.to_string();
        let Some(mesh) = doc.get_mesh(&id) else {
            return error_dict("MISSING_MESH", &id);
        };
        let mut out = Dictionary::new();
        out.set("mesh", &crate::conversions::structured_to_dict(mesh));
        out.set(
            "binding",
            &doc.binding_for_mesh(&id)
                .map(|b| crate::conversions::structured_to_dict(b).to_variant())
                .unwrap_or_else(Variant::nil),
        );
        let mut bindings = Array::new();
        for bid in doc.blend_binding_order() {
            let b = doc.get_blend_binding(bid).unwrap();
            if b.target_id == id {
                bindings.push(&crate::conversions::structured_to_dict(b));
            }
        }
        out.set("blend_bindings", &bindings);
        let mut glues = Array::new();
        for g in doc.glues_for_mesh(&id) {
            glues.push(&crate::conversions::structured_to_dict(g));
        }
        out.set("glues", &glues);
        let mut mapping = Array::new();
        for &id in &mesh.vertex_ids {
            let mut pair = Array::new();
            pair.push(id as i64);
            pair.push(id as i64);
            mapping.push(&pair);
        }
        out.set("vertex_mapping", &mapping);
        out
    }

    #[func]
    pub fn set_mesh_keyform(
        &mut self,
        binding_id: GString,
        keys: PackedFloat32Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let mut key_vec = Vec::with_capacity(keys.len());
        for i in 0..keys.len() {
            key_vec.push(keys[i]);
        }
        let mut form = kasane_core::types::MeshKeyform {
            keys: key_vec,
            positions: packed_to_vectors(&positions),
            appearance: Default::default(),
            draw_order: None,
        };
        if let Some(b) = self.session.document().get_binding(&binding_id.to_string()) {
            for old in &b.keyforms {
                if old.keys == form.keys {
                    form.appearance = old.appearance;
                    form.draw_order = old.draw_order;
                    break;
                }
            }
        }
        let edit = self
            .session
            .document_mut()
            .set_mesh_keyform(&binding_id.to_string(), form);
        self.apply(edit)
    }

    #[func]
    pub fn erase_object(&mut self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let edit = self.session.document_mut().erase_object(&id.to_string());
        if edit.status.is_ok() {
            *self.object_epochs.entry(id.to_string()).or_default() += 1;
        }
        self.apply(edit)
    }

    #[func]
    pub fn create_rotation(
        &mut self,
        id: GString,
        name: GString,
        center: Vector2,
        angle: f64,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let t = Transform {
            id: id.to_string(),
            runtime_id: format!("Rotation_{}", id.to_string().replace('-', "")),
            name: name.to_string(),
            part_id: String::new(),
            parent_id: String::new(),
            kind: TransformKind::Rotation,
            base_angle: 0.0,
            rotation: RotationPose {
                origin: Vec2::new(center.x, center.y).into(),
                angle: angle as f32,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
            rows: 0,
            columns: 0,
            quad: false,
            enabled: true,
            points: Vec::new(),
            appearance: Default::default(),
        };
        let edit = self.session.document_mut().create_transform(t);
        self.apply(edit)
    }

    #[func]
    pub fn create_warp(
        &mut self,
        id: GString,
        name: GString,
        origin: Vector2,
        size: Vector2,
        columns: i64,
        rows: i64,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        if columns < 1 || rows < 1 || columns > 16 || rows > 16 {
            return error_dict("INVALID_WARP", "Warp supports 1–16 cells per axis.");
        }
        let mut points = Vec::new();
        for y in 0..=rows {
            for x in 0..=columns {
                points.push(Vec2::new(
                    origin.x + size.x * (x as f32 / columns as f32),
                    origin.y + size.y * (y as f32 / rows as f32),
                ));
            }
        }
        let t = Transform {
            id: id.to_string(),
            runtime_id: format!("Warp_{}", id.to_string().replace('-', "")),
            name: name.to_string(),
            part_id: String::new(),
            parent_id: String::new(),
            kind: TransformKind::Warp,
            base_angle: 0.0,
            rotation: Default::default(),
            rows: rows as u32,
            columns: columns as u32,
            quad: false,
            enabled: true,
            points,
            appearance: Default::default(),
        };
        let edit = self.session.document_mut().create_transform(t);
        self.apply(edit)
    }

    #[func]
    pub fn set_rotation(&mut self, id: GString, center: Vector2, angle: f64) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let Some(old) = self.session.document().get_transform(&id.to_string()) else {
            return error_dict("MISSING_TRANSFORM", &id.to_string());
        };
        if old.kind != TransformKind::Rotation {
            return error_dict("WRONG_TRANSFORM_KIND", "Use a Rotation deformer.");
        }
        let mut t = old.clone();
        t.rotation.origin = Vec2::new(center.x, center.y).into();
        t.rotation.angle = angle as f32;
        let edit = self.session.document_mut().replace_transform(t);
        self.apply(edit)
    }

    #[func]
    pub fn set_warp_points(&mut self, id: GString, points: PackedVector2Array) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let Some(old) = self.session.document().get_transform(&id.to_string()) else {
            return error_dict("MISSING_TRANSFORM", &id.to_string());
        };
        if old.kind != TransformKind::Warp {
            return error_dict("WRONG_TRANSFORM_KIND", "Use a Warp deformer.");
        }
        let mut t = old.clone();
        t.points = packed_to_vectors(&points);
        let edit = self.session.document_mut().replace_transform(t);
        self.apply(edit)
    }

    #[func]
    pub fn set_deform_parent(&mut self, id: GString, parent: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let id_str = id.to_string();
        let parent_str = parent.to_string();
        if let Some(old) = self.session.document().get_transform(&id_str) {
            let mut t = old.clone();
            t.parent_id = parent_str;
            let edit = self.session.document_mut().replace_transform(t);
            return self.apply(edit);
        }
        if let Some(old) = self.session.document().get_mesh(&id_str) {
            let mut m = old.clone();
            m.deformer_id = parent_str;
            let edit = self.session.document_mut().replace_mesh(m);
            return self.apply(edit);
        }
        error_dict("MISSING_OBJECT", &id_str)
    }

    #[func]
    pub fn set_organization_parent(&mut self, id: GString, parent: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let id_str = id.to_string();
        let parent_str = parent.to_string();
        if let Some(old) = self.session.document().get_part(&id_str) {
            let mut p = old.clone();
            p.parent_id = parent_str;
            let edit = self.session.document_mut().replace_part(p);
            return self.apply(edit);
        }
        if let Some(old) = self.session.document().get_transform(&id_str) {
            let mut t = old.clone();
            t.part_id = parent_str;
            let edit = self.session.document_mut().replace_transform(t);
            return self.apply(edit);
        }
        if let Some(old) = self.session.document().get_mesh(&id_str) {
            let mut m = old.clone();
            m.part_id = parent_str;
            let edit = self.session.document_mut().replace_mesh(m);
            return self.apply(edit);
        }
        error_dict("MISSING_OBJECT", &id_str)
    }

    #[func]
    pub fn get_deformer_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let Some(t) = self.session.document().get_transform(&id.to_string()) else {
            return error_dict("MISSING_DEFORMER", "Deformer does not exist.");
        };
        let mut out = status_to_dict(&Status::ok());
        out.set("id", &id);
        out.set("name", t.name.as_str());
        let kind_str = match t.kind {
            TransformKind::Rotation => "rotation",
            TransformKind::Warp => "warp",
        };
        out.set("kind", kind_str);
        out.set("deform_parent", t.parent_id.as_str());
        out.set("organization_parent", t.part_id.as_str());
        out.set("revision", self.session.document().revision() as i64);
        if t.kind == TransformKind::Rotation {
            out.set(
                "center",
                Vector2::new(t.rotation.origin.x as f32, t.rotation.origin.y as f32),
            );
            out.set("angle_degrees", t.rotation.angle as f64);
        } else {
            out.set(
                "origin",
                Vector2::new(
                    t.points.first().map(|p| p.x).unwrap_or(0.0),
                    t.points.first().map(|p| p.y).unwrap_or(0.0),
                ),
            );
            out.set("size", Vector2::ZERO);
            out.set("columns", t.columns as i64);
            out.set("rows", t.rows as i64);
            out.set("control_points", &vectors_to_packed(&t.points));
        }
        out
    }

    #[func]
    pub fn get_deformer(&self, id: GString) -> Option<Gd<KasaneDeformerData>> {
        if !is_main_thread() {
            return None;
        }
        self.session.document().get_transform(&id.to_string())?;
        let mut handle = Gd::<KasaneDeformerData>::default();
        handle.bind_mut().attach(
            self.base().instance_id().to_i64() as u64,
            self.generation,
            self.object_epoch(&id.to_string()),
            id,
        );
        Some(handle)
    }

    #[func]
    pub fn evaluate_mesh(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let mut frame = DrawableFrame::default();
        let status = self.evaluate(&mut frame);
        if !status.is_ok() {
            return status_to_dict(&status);
        }
        let id_str = id.to_string();
        for d in &frame.drawables {
            if d.id == id_str {
                let mut out = status_to_dict(&Status::ok());
                out.set("id", &id);
                let positions = vectors_to_packed(&d.positions);
                let uvs = vectors_to_packed(&d.uvs);
                let indices = ids_to_packed(&d.indices);
                out.set("positions", &positions);
                out.set("uvs", &uvs);
                out.set("indices", &indices);
                out.set("visible", d.visible);
                out.set("opacity", d.opacity);
                out.set("draw_order", d.draw_order);
                out.set("render_order", d.render_order);
                return out;
            }
        }
        error_dict("MISSING_MESH", &id_str)
    }

    #[func]
    pub fn get_frame(&self) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let mut frame = DrawableFrame::default();
        let status = self.evaluate(&mut frame);
        let mut out = status_to_dict(&status);
        if !status.is_ok() {
            return out;
        }
        out.set("revision", frame.source_revision as i64);
        out.set("coordinate_units", "runtime");
        let mut canvas = Dictionary::new();
        canvas.set("width", frame.canvas.width);
        canvas.set("height", frame.canvas.height);
        canvas.set(
            "origin",
            Vector2::new(frame.canvas.origin.x, frame.canvas.origin.y),
        );
        canvas.set("pixels_per_unit", frame.canvas.pixels_per_unit);
        canvas.set("flag", i64::from(frame.canvas.flag));
        out.set("canvas", &canvas);
        let mut drawables = Array::new();
        for d in &frame.drawables {
            let mut item = Dictionary::new();
            item.set("id", d.id.as_str());
            item.set("runtime_id", d.runtime_id.as_str());
            item.set("part_id", d.part_id.as_str());
            let positions = vectors_to_packed(&d.positions);
            let uvs = vectors_to_packed(&d.uvs);
            let indices = ids_to_packed(&d.indices);
            item.set("positions", &positions);
            item.set("uvs", &uvs);
            item.set("indices", &indices);
            item.set("texture_asset_id", d.texture_asset_id.as_str());
            item.set("texture_slot", d.texture_slot as i64);
            item.set("draw_order", d.draw_order);
            item.set("render_order", d.render_order);
            item.set("opacity", d.opacity);
            item.set("enabled", d.enabled);
            item.set("visible", d.visible);
            item.set("double_sided", d.double_sided);
            item.set("inverted_mask", d.inverted_mask);
            let blend_int = match d.blend_mode {
                BlendMode::Normal => 0,
                BlendMode::Additive => 1,
                BlendMode::Multiplicative => 2,
            };
            item.set("blend_mode", blend_int);
            item.set(
                "raw_blend_mode",
                d.raw_blend_mode.map(i64::from).unwrap_or(-1),
            );
            let mut masks = PackedStringArray::new();
            for m in &d.masks {
                masks.push(&GString::from(m.as_str()));
            }
            item.set("masks", &masks);
            item.set(
                "multiply_color",
                Color::from_rgba(
                    d.multiply_color[0],
                    d.multiply_color[1],
                    d.multiply_color[2],
                    d.multiply_color[3],
                ),
            );
            item.set(
                "screen_color",
                Color::from_rgba(
                    d.screen_color[0],
                    d.screen_color[1],
                    d.screen_color[2],
                    d.screen_color[3],
                ),
            );
            drawables.push(&item);
        }
        let mut offscreens = Array::new();
        for offscreen in &frame.offscreens {
            let mut item = Dictionary::new();
            item.set("id", offscreen.id.as_str());
            item.set("runtime_id", offscreen.runtime_id.as_str());
            item.set("owner_part_id", offscreen.owner_part_id.as_str());
            item.set(
                "parent_offscreen_id",
                offscreen.parent_offscreen_id.as_deref().unwrap_or(""),
            );
            item.set("render_order", offscreen.render_order);
            item.set("opacity", offscreen.opacity);
            item.set("enabled", offscreen.enabled);
            item.set("blend_mode", i64::from(offscreen.blend_mode));
            item.set("inverted_mask", offscreen.flags & 8 != 0);
            let mut masks = PackedStringArray::new();
            for mask in &offscreen.masks {
                masks.push(&GString::from(mask.as_str()));
            }
            item.set("masks", &masks);
            item.set(
                "multiply_color",
                Color::from_rgba(
                    offscreen.multiply_color[0],
                    offscreen.multiply_color[1],
                    offscreen.multiply_color[2],
                    offscreen.multiply_color[3],
                ),
            );
            item.set(
                "screen_color",
                Color::from_rgba(
                    offscreen.screen_color[0],
                    offscreen.screen_color[1],
                    offscreen.screen_color[2],
                    offscreen.screen_color[3],
                ),
            );
            offscreens.push(&item);
        }
        let mut render_plan = Array::new();
        for command in &frame.render_plan {
            let mut item = Dictionary::new();
            match command {
                kasane_core::evaluation::RenderCommand::BeginOffscreen { offscreen_id } => {
                    item.set("command", "begin_offscreen");
                    item.set("id", offscreen_id.as_str());
                }
                kasane_core::evaluation::RenderCommand::DrawMesh { mesh_id } => {
                    item.set("command", "draw_mesh");
                    item.set("id", mesh_id.as_str());
                }
                kasane_core::evaluation::RenderCommand::EndOffscreen { offscreen_id } => {
                    item.set("command", "end_offscreen");
                    item.set("id", offscreen_id.as_str());
                }
            }
            render_plan.push(&item);
        }
        let mut parameters = Array::new();
        for p in &frame.parameters {
            let mut value = Dictionary::new();
            value.set("id", p.id.as_str());
            value.set("requested", p.requested);
            value.set("value", p.value);
            value.set("clamped", p.clamped);
            parameters.push(&value);
        }
        out.set("drawables", &drawables);
        out.set("offscreens", &offscreens);
        out.set("render_plan", &render_plan);
        out.set("parameters", &parameters);
        out
    }

    #[func]
    pub fn set_preview_values(&mut self, values: Dictionary) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let mut next = HashMap::new();
        for (k, v) in values.iter_shared() {
            let Ok(k_str) = k.try_to::<GString>() else {
                return error_dict(
                    "INVALID_FIELD",
                    "Preview requires parameter IDs and numeric values.",
                );
            };
            let val = if let Ok(f) = v.try_to::<f64>() {
                f as f32
            } else if let Ok(i) = v.try_to::<i64>() {
                i as f32
            } else {
                return error_dict(
                    "INVALID_FIELD",
                    "Preview requires parameter IDs and numeric values.",
                );
            };
            if !val.is_finite() {
                return error_dict(
                    "INVALID_FIELD",
                    "Preview requires parameter IDs and numeric values.",
                );
            }
            next.insert(k_str.to_string(), val);
        }
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(self.session.document(), &next, &mut frame);
        if !status.is_ok() {
            return status_to_dict(&status);
        }
        self.preview_values = next;
        self.base_mut().emit_signal("preview_changed", &[]);
        self.get_frame()
    }

    #[func]
    pub fn write_part(
        &mut self,
        data: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let p = match part_from_dict(&data) {
            Ok(p) => p,
            Err(s) => return status_to_dict(&s),
        };
        let edit = if replace {
            self.session.document_mut().replace_part(p)
        } else {
            self.session.document_mut().create_part(p)
        };
        self.apply(edit)
    }

    #[func]
    pub fn write_transform(
        &mut self,
        data: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let t = match transform_from_dict(&data) {
            Ok(t) => t,
            Err(s) => return status_to_dict(&s),
        };
        let edit = if replace {
            self.session.document_mut().replace_transform(t)
        } else {
            self.session.document_mut().create_transform(t)
        };
        self.apply(edit)
    }

    #[func]
    pub fn write_scene_binding(
        &mut self,
        data: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let b = match scene_binding_from_dict(&data) {
            Ok(b) => b,
            Err(s) => return status_to_dict(&s),
        };
        let edit = if replace {
            self.session.document_mut().replace_scene_binding(b)
        } else {
            self.session.document_mut().create_scene_binding(b)
        };
        self.apply(edit)
    }

    #[func]
    pub fn set_mesh_properties(&mut self, id: GString, data: Dictionary) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let Some(old) = self.session.document().get_mesh(&id.to_string()) else {
            return error_dict("MISSING_MESH", &id.to_string());
        };
        let mut m = old.clone();
        m.masks.clear();
        m.draw_order = None;
        if let Err(s) = mesh_properties_from_dict(&data, &mut m) {
            return status_to_dict(&s);
        }
        let edit = self.session.document_mut().replace_mesh(m);
        self.apply(edit)
    }

    #[func]
    pub fn set_vertex_positions(
        &mut self,
        mesh_id: GString,
        vertex_ids: PackedInt64Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let vertices = match packed_to_ids(&vertex_ids) {
            Ok(v) => v,
            Err(s) => return status_to_dict(&s),
        };
        let edit = self.session.document_mut().set_vertex_positions(
            &mesh_id.to_string(),
            &vertices,
            &packed_to_vectors(&positions),
        );
        self.apply(edit)
    }

    #[func]
    pub fn rename_mesh(&mut self, id: GString, name: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let edit = self
            .session
            .document_mut()
            .rename_mesh(&id.to_string(), name.to_string());
        self.apply(edit)
    }

    #[func]
    pub fn begin_transaction(&mut self) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let s = self.session.document_mut().begin_transaction();
        status_to_dict(&s)
    }

    #[func]
    pub fn stage_vertex_positions(
        &mut self,
        mesh_id: GString,
        vertex_ids: PackedInt64Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let vertices = match packed_to_ids(&vertex_ids) {
            Ok(v) => v,
            Err(s) => return status_to_dict(&s),
        };
        let update = VertexPositionUpdate {
            mesh_id: mesh_id.to_string(),
            vertex_ids: vertices,
            positions: packed_to_vectors(&positions),
        };
        let s = self.session.document_mut().stage_vertex_positions(update);
        status_to_dict(&s)
    }

    #[func]
    pub fn commit_transaction(&mut self) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let edit = self.session.document_mut().commit_transaction();
        self.apply(edit)
    }

    #[func]
    pub fn cancel_transaction(&mut self) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let s = self.session.document_mut().cancel_transaction();
        status_to_dict(&s)
    }

    #[func]
    pub fn commit_vertex_updates(&mut self, updates: Array, expected_revision: i64) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        if expected_revision < 0 {
            return error_dict("INVALID_REVISION", "Expected revision must be nonnegative.");
        }
        let mut batch = Vec::with_capacity(updates.len());
        for i in 0..updates.len() {
            let item_val = updates.at(i);
            let Ok(item) = item_val.try_to::<Dictionary>() else {
                return error_dict("INVALID_FIELD", "Each update must be a Dictionary.");
            };
            if !item.contains_key("mesh_id")
                || !item.contains_key("vertex_ids")
                || !item.contains_key("positions")
            {
                return error_dict(
                    "INVALID_FIELD",
                    "Update requires mesh_id, vertex_ids and positions.",
                );
            }
            let Ok(mesh_id) = item.get("mesh_id").unwrap().try_to::<GString>() else {
                return error_dict("INVALID_FIELD", "mesh_id must be a string");
            };
            let Ok(vertex_ids_raw) = item.get("vertex_ids").unwrap().try_to::<PackedInt64Array>()
            else {
                return error_dict("INVALID_FIELD", "vertex_ids must be PackedInt64Array");
            };
            let Ok(positions_raw) = item
                .get("positions")
                .unwrap()
                .try_to::<PackedVector2Array>()
            else {
                return error_dict("INVALID_FIELD", "positions must be PackedVector2Array");
            };
            let vertex_ids = match packed_to_ids(&vertex_ids_raw) {
                Ok(v) => v,
                Err(s) => return status_to_dict(&s),
            };
            batch.push(VertexPositionUpdate {
                mesh_id: mesh_id.to_string(),
                vertex_ids,
                positions: packed_to_vectors(&positions_raw),
            });
        }
        let edit = self
            .session
            .document_mut()
            .apply_vertex_position_updates_at_revision(&batch, expected_revision as u64);
        self.apply(edit)
    }

    #[func]
    pub fn get_asset_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let Some(asset) = self.session.document().get_asset(&id.to_string()) else {
            return error_dict("MISSING_ASSET", "Asset does not exist.");
        };
        let mut out = status_to_dict(&Status::ok());
        out.set("id", asset.id.as_str());
        out.set("name", asset.name.as_str());
        out.set("source", asset.source.as_str());
        out.set("sha256", asset.sha256.as_str());
        out.set("width", asset.width as i64);
        out.set("height", asset.height as i64);
        out.set("revision", self.session.document().revision() as i64);
        out
    }

    #[func]
    pub fn get_mesh_snapshot(&self, id: GString) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let Some(mesh) = self.session.document().get_mesh(&id.to_string()) else {
            return error_dict("MISSING_MESH", "Mesh does not exist.");
        };
        let mut out = status_to_dict(&Status::ok());
        out.set("id", mesh.id.as_str());
        out.set("name", mesh.name.as_str());
        out.set("texture_asset_id", mesh.texture_asset_id.as_str());
        out.set("runtime_id", mesh.runtime_id.as_str());
        let props = dict_from_mesh_properties(mesh);
        out.set("properties", &props);
        out.set("deform_parent", mesh.deformer_id.as_str());
        out.set("organization_parent", mesh.part_id.as_str());
        let vertex_ids = ids_to_packed(&mesh.vertex_ids);
        let base_positions = vectors_to_packed(&mesh.base_positions);
        let uvs = vectors_to_packed(&mesh.uvs);
        out.set("vertex_ids", &vertex_ids);
        out.set("base_positions", &base_positions);
        out.set("uvs", &uvs);
        let mut tri_flat = Vec::with_capacity(mesh.triangles.len() * 3);
        for t in &mesh.triangles {
            tri_flat.extend_from_slice(t);
        }
        let triangles = ids_to_packed(&tri_flat);
        out.set("triangles", &triangles);
        out.set("revision", self.session.document().revision() as i64);
        out
    }

    /// Lightweight execution/observation metadata; does not copy model keyforms.
    #[func]
    pub fn get_document_state(&self) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document requires the main thread.");
        }
        let doc = self.session.document();
        let mut out = status_to_dict(&Status::ok());
        out.set("id", doc.id());
        out.set("initialized", doc.initialized());
        out.set("generation", self.generation as i64);
        out.set("revision", doc.revision() as i64);
        out.set("modified", doc.modified());
        out.set("transaction_active", doc.transaction_active());
        out.set("path", self.session.manifest().to_string_lossy().as_ref());
        out.set(
            "canvas_size",
            Vector2::new(doc.canvas().width, doc.canvas().height),
        );
        out.set(
            "canvas_origin",
            Vector2::new(doc.canvas().origin.x, doc.canvas().origin.y),
        );
        out.set("pixels_per_unit", doc.canvas().pixels_per_unit as f64);
        out
    }

    #[func]
    pub fn get_document_summary(&self) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let doc = self.session.document();
        let mut out = Dictionary::new();
        out.set("schema_version", 1i64);
        out.set("initialized", doc.initialized());
        out.set("id", doc.id());
        out.set(
            "canvas_size",
            Vector2::new(doc.canvas().width, doc.canvas().height),
        );
        out.set("revision", doc.revision() as i64);
        out.set("asset_count", doc.asset_order().len() as i64);
        let mut assets = Array::new();
        for id in doc.asset_order() {
            assets.push(&self.get_asset_snapshot(GString::from(id.as_str())));
        }
        out.set("assets", &assets);
        out.set("modified", doc.modified());
        out.set("path", self.session.manifest().to_string_lossy().as_ref());
        out.set("transaction_active", doc.transaction_active());
        out.set("generation", self.generation as i64);

        let mut meshes = Array::new();
        for id in doc.mesh_order() {
            if let Some(m) = doc.get_mesh(id) {
                let mut item = Dictionary::new();
                item.set("id", m.id.as_str());
                item.set("name", m.name.as_str());
                item.set("vertex_count", m.vertex_ids.len() as i64);
                item.set("triangle_count", m.triangles.len() as i64);
                meshes.push(&item);
            }
        }
        out.set("meshes", &meshes);

        let mut deformers = Array::new();
        for id in doc.transform_order() {
            deformers.push(&self.get_deformer_snapshot(GString::from(id.as_str())));
        }
        out.set("deformers", &deformers);

        let mut parameters = Array::new();
        for id in doc.parameter_order() {
            if let Some(p) = doc.get_parameter(id) {
                parameters.push(&dict_from_parameter(p));
            }
        }
        out.set("parameters", &parameters);

        let mut bindings = Array::new();
        for id in doc.binding_order() {
            if let Some(b) = doc.get_binding(id) {
                bindings.push(&dict_from_binding(b));
            }
        }
        out.set("bindings", &bindings);

        let mut parts = Array::new();
        for id in doc.part_order() {
            if let Some(p) = doc.get_part(id) {
                parts.push(&dict_from_part(p));
            }
        }
        out.set("parts", &parts);

        let mut transforms = Array::new();
        for id in doc.transform_order() {
            if let Some(t) = doc.get_transform(id) {
                transforms.push(&dict_from_transform(t));
            }
        }
        out.set("transforms", &transforms);

        let mut scene_bindings = Array::new();
        for id in doc.scene_binding_order() {
            if let Some(sb) = doc.get_scene_binding(id) {
                scene_bindings.push(&dict_from_scene_binding(sb));
            }
        }
        out.set("scene_bindings", &scene_bindings);
        let mut items = Array::new();
        for id in doc.blend_key_table_order() {
            items.push(&crate::conversions::structured_to_dict(
                doc.get_blend_key_table(id).unwrap(),
            ));
        }
        out.set("blend_key_tables", &items);
        let mut items = Array::new();
        for id in doc.blend_constraint_order() {
            items.push(&crate::conversions::structured_to_dict(
                doc.get_blend_constraint(id).unwrap(),
            ));
        }
        out.set("blend_constraints", &items);
        let mut items = Array::new();
        for id in doc.blend_binding_order() {
            items.push(&crate::conversions::structured_to_dict(
                doc.get_blend_binding(id).unwrap(),
            ));
        }
        out.set("blend_bindings", &items);
        let mut items = Array::new();
        for id in doc.glue_order() {
            items.push(&crate::conversions::structured_to_dict(
                doc.get_glue(id).unwrap(),
            ));
        }
        out.set("glues", &items);
        let mut items = Array::new();
        for id in doc.offscreen_order() {
            items.push(&crate::conversions::structured_to_dict(doc.get_offscreen(id).unwrap()));
        }
        out.set("offscreens", &items);

        out.set(
            "canvas_origin",
            Vector2::new(doc.canvas().origin.x, doc.canvas().origin.y),
        );
        out.set("pixels_per_unit", doc.canvas().pixels_per_unit as f64);
        out
    }

    #[func]
    pub fn capture_state(&self) -> Option<Gd<KasaneDocumentState>> {
        if !is_main_thread() {
            return None;
        }
        if self.session.document().transaction_active() {
            return None;
        }
        let mut state = Gd::<KasaneDocumentState>::default();
        let inst_id = self.base().instance_id().to_i64() as u64;
        let mut b = state.bind_mut();
        b.document = self.session.document().clone();
        b.owner_id = inst_id;
        b.generation = self.generation;
        drop(b);
        Some(state)
    }

    #[func]
    pub fn restore_state(&mut self, state: Option<Gd<KasaneDocumentState>>) -> Dictionary {
        if !is_main_thread() {
            return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
        }
        let Some(s) = state else {
            return error_dict("STALE_STATE", "State belongs to another document session.");
        };
        let b = s.bind();
        let inst_id = self.base().instance_id().to_i64() as u64;
        if b.owner_id != inst_id || b.generation != self.generation {
            return error_dict("STALE_STATE", "State belongs to another document session.");
        }
        if self.session.document().transaction_active() {
            return error_dict(
                "TRANSACTION_ACTIVE",
                "Commit or cancel the transaction first.",
            );
        }
        let doc = self.session.document();
        let removed: Vec<String> = doc
            .mesh_order()
            .iter()
            .chain(doc.transform_order())
            .filter(|id| !b.document.contains_id(id))
            .cloned()
            .collect();
        for id in removed {
            *self.object_epochs.entry(id).or_default() += 1;
        }
        self.session.document_mut().restore_from(&b.document);
        drop(b);
        self.preview_values.clear();
        let mut out = status_to_dict(&Status::ok());
        out.set("revision", self.session.document().revision() as i64);
        self.base_mut().emit_signal("changed", &[out.to_variant()]);
        out
    }
}
