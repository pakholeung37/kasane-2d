mod blendshapes;
mod deformers;
mod glue;
mod history;
mod inspection;
mod lifecycle;
mod meshes;
mod offscreen;
mod parameters;
mod parts;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use godot::prelude::*;
use kasane_core::evaluation::DrawableFrame;
use kasane_core::preview::PreviewState;
use kasane_core::types::{ChangeKind, EditResult, Status};
use kasane_project::store::DocumentSession;

use crate::conversions::{status_to_dict, Array, Dictionary};
use crate::deformer_data::KasaneDeformerData;
use crate::mesh_data::KasaneMeshData;

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct KasaneDocumentBridge {
    base: Base<RefCounted>,
    pub(super) session: DocumentSession,
    pub(super) generation: u64,
    pub(super) preview: RefCell<PreviewState>,
    pub(super) object_epochs: HashMap<String, u64>,
}

#[godot_api]
impl IRefCounted for KasaneDocumentBridge {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            session: DocumentSession::new(),
            generation: 1,
            preview: RefCell::new(PreviewState::default()),
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
        self.preview.get_mut().reset();
        self.object_epochs.clear();
    }

    pub fn object_epoch(&self, id: &str) -> u64 {
        *self.object_epochs.get(id).unwrap_or(&0)
    }

    pub fn session(&self) -> &DocumentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut DocumentSession {
        self.preview.get_mut().invalidate();
        &mut self.session
    }

    pub fn evaluated_frame(&self) -> Result<Arc<DrawableFrame>, Status> {
        self.preview
            .borrow_mut()
            .frame(self.session.document(), self.generation)
    }

    pub fn apply(&mut self, edit: EditResult) -> Dictionary {
        self.session.record_edit(&edit);
        self.publish_edit(edit)
    }

    pub(super) fn publish_edit(&mut self, edit: EditResult) -> Dictionary {
        let mut out = status_to_dict(&edit.status);
        out.set("revision", edit.changes.revision as i64);
        let kind_str = match edit.changes.kind {
            ChangeKind::Structure => "structure",
            ChangeKind::Positions => "positions",
            ChangeKind::Metadata => "metadata",
            ChangeKind::Resources => "resources",
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
        out.set("history", &history::get_history_state(self));
        if let Some(notice) = self.session.history().notice() {
            out.set("history_warning", notice);
        }

        if !edit.status.is_ok() {
            return out;
        }

        self.preview
            .get_mut()
            .retain_parameters(self.session.document());

        if edit.changes.kind != ChangeKind::None {
            self.base_mut().emit_signal("changed", &[out.to_variant()]);
        }
        out
    }

    // --- Lifecycle ---

    #[func]
    pub fn initialize(
        &mut self,
        id: GString,
        canvas_size: Vector2,
        #[opt(default = Vector2::ZERO)] origin: Vector2,
        #[opt(default = 1.0)] pixels_per_unit: f64,
    ) -> Dictionary {
        lifecycle::initialize(self, id, canvas_size, origin, pixels_per_unit)
    }

    #[func]
    pub fn new_project(
        &mut self,
        id: GString,
        canvas_size: Vector2,
        #[opt(default = Vector2::ZERO)] origin: Vector2,
        #[opt(default = 1.0)] pixels_per_unit: f64,
    ) -> Dictionary {
        lifecycle::new_project(self, id, canvas_size, origin, pixels_per_unit)
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
        lifecycle::add_image_asset(self, id, name, source, width, height)
    }

    #[func]
    pub fn get_asset_snapshot(&self, id: GString) -> Dictionary {
        lifecycle::get_asset_snapshot(self, id)
    }

    // --- History & Actions ---

    #[func]
    pub fn begin_transaction(&mut self) -> Dictionary {
        history::begin_transaction(self)
    }

    #[func]
    pub fn commit_transaction(&mut self) -> Dictionary {
        history::commit_transaction(self)
    }

    #[func]
    pub fn cancel_transaction(&mut self) -> Dictionary {
        history::cancel_transaction(self)
    }

    #[func]
    pub fn get_history_state(&self) -> Dictionary {
        history::get_history_state(self)
    }

    #[func]
    pub fn begin_action(&mut self, label: GString) -> Dictionary {
        history::begin_action(self, label)
    }

    #[func]
    pub fn end_action(&mut self) -> Dictionary {
        history::end_action(self)
    }

    #[func]
    pub fn cancel_action(&mut self) -> Dictionary {
        history::cancel_action(self)
    }

    #[func]
    pub fn undo(&mut self) -> Dictionary {
        history::undo(self)
    }

    #[func]
    pub fn redo(&mut self) -> Dictionary {
        history::redo(self)
    }

    // --- Meshes ---

    #[func]
    pub fn create_mesh(&mut self, description: Dictionary) -> Dictionary {
        meshes::create_mesh(self, description)
    }

    #[func]
    pub fn replace_mesh(&mut self, description: Dictionary) -> Dictionary {
        meshes::replace_mesh(self, description)
    }

    #[func]
    pub fn replace_mesh_with_keyforms(
        &mut self,
        description: Dictionary,
        keyforms: Dictionary,
    ) -> Dictionary {
        meshes::replace_mesh_with_keyforms(self, description, keyforms)
    }

    #[func]
    pub fn get_mesh(&self, id: GString) -> Option<Gd<KasaneMeshData>> {
        meshes::get_mesh(self, id)
    }

    #[func]
    pub fn replace_mesh_topology(&mut self, description: Dictionary) -> Dictionary {
        meshes::replace_mesh_topology(self, description)
    }

    #[func]
    pub fn get_mesh_topology_snapshot(&self, id: GString) -> Dictionary {
        meshes::get_mesh_topology_snapshot(self, id)
    }

    #[func]
    pub fn set_mesh_keyform(
        &mut self,
        binding_id: GString,
        keys: PackedFloat32Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        meshes::set_mesh_keyform(self, binding_id, keys, positions)
    }

    #[func]
    pub fn set_mesh_properties(&mut self, id: GString, data: Dictionary) -> Dictionary {
        meshes::set_mesh_properties(self, id, data)
    }

    #[func]
    pub fn set_vertex_positions(
        &mut self,
        mesh_id: GString,
        vertex_ids: PackedInt64Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        meshes::set_vertex_positions(self, mesh_id, vertex_ids, positions)
    }

    #[func]
    pub fn rename_mesh(&mut self, id: GString, name: GString) -> Dictionary {
        meshes::rename_mesh(self, id, name)
    }

    #[func]
    pub fn stage_vertex_positions(
        &mut self,
        mesh_id: GString,
        vertex_ids: PackedInt64Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        meshes::stage_vertex_positions(self, mesh_id, vertex_ids, positions)
    }

    #[func]
    pub fn commit_vertex_updates(&mut self, updates: Array, expected_revision: i64) -> Dictionary {
        meshes::commit_vertex_updates(self, updates, expected_revision)
    }

    #[func]
    pub fn get_mesh_snapshot(&self, id: GString) -> Dictionary {
        meshes::get_mesh_snapshot(self, id)
    }

    // --- Parameters & Preview ---

    #[func]
    pub fn create_parameter(&mut self, description: Dictionary) -> Dictionary {
        parameters::create_parameter(self, description)
    }

    #[func]
    pub fn replace_parameter(&mut self, description: Dictionary) -> Dictionary {
        parameters::replace_parameter(self, description)
    }

    #[func]
    pub fn get_parameter(&self, id: GString) -> Dictionary {
        parameters::get_parameter(self, id)
    }

    #[func]
    pub fn set_preview_values(&mut self, values: Dictionary) -> Dictionary {
        parameters::set_preview_values(self, values)
    }

    #[func]
    pub fn set_preview_parameter(&mut self, id: GString, value: f64) -> Dictionary {
        parameters::set_preview_parameter(self, id, value)
    }

    #[func]
    pub fn reset_preview_values(&mut self) -> Dictionary {
        parameters::reset_preview_values(self)
    }

    #[func]
    pub fn get_parameter_samples(&self) -> Dictionary {
        parameters::get_parameter_samples(self)
    }

    // --- Deformers & Transforms ---

    #[func]
    pub fn create_rotation(
        &mut self,
        id: GString,
        name: GString,
        center: Vector2,
        angle: f64,
    ) -> Dictionary {
        deformers::create_rotation(self, id, name, center, angle)
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
        deformers::create_warp(self, id, name, origin, size, columns, rows)
    }

    #[func]
    pub fn set_rotation(&mut self, id: GString, center: Vector2, angle: f64) -> Dictionary {
        deformers::set_rotation(self, id, center, angle)
    }

    #[func]
    pub fn set_warp_points(&mut self, id: GString, points: PackedVector2Array) -> Dictionary {
        deformers::set_warp_points(self, id, points)
    }

    #[func]
    pub fn set_deform_parent(&mut self, id: GString, parent: GString) -> Dictionary {
        deformers::set_deform_parent(self, id, parent)
    }

    #[func]
    pub fn set_organization_parent(&mut self, id: GString, parent: GString) -> Dictionary {
        deformers::set_organization_parent(self, id, parent)
    }

    #[func]
    pub fn get_deformer_snapshot(&self, id: GString) -> Dictionary {
        deformers::get_deformer_snapshot(self, id)
    }

    #[func]
    pub fn get_deformer(&self, id: GString) -> Option<Gd<KasaneDeformerData>> {
        deformers::get_deformer(self, id)
    }

    #[func]
    pub fn write_transform(
        &mut self,
        data: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        deformers::write_transform(self, data, replace)
    }

    // --- Parts & Bindings ---

    #[func]
    pub fn write_part(
        &mut self,
        data: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        parts::write_part(self, data, replace)
    }

    #[func]
    pub fn replace_part_binding_with_offscreen(
        &mut self,
        binding: Dictionary,
        offscreen: Dictionary,
    ) -> Dictionary {
        parts::replace_part_binding_with_offscreen(self, binding, offscreen)
    }

    #[func]
    pub fn write_scene_binding(
        &mut self,
        data: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        parts::write_scene_binding(self, data, replace)
    }

    // --- Blendshapes ---

    #[func]
    pub fn write_binding(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        blendshapes::write_binding(self, description, replace)
    }

    #[func]
    pub fn write_blend_key_table(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        blendshapes::write_blend_key_table(self, description, replace)
    }

    #[func]
    pub fn get_blend_key_table_snapshot(&self, id: GString) -> Dictionary {
        blendshapes::get_blend_key_table_snapshot(self, id)
    }

    #[func]
    pub fn write_blend_constraint(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        blendshapes::write_blend_constraint(self, description, replace)
    }

    #[func]
    pub fn get_blend_constraint_snapshot(&self, id: GString) -> Dictionary {
        blendshapes::get_blend_constraint_snapshot(self, id)
    }

    #[func]
    pub fn write_blend_binding(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        blendshapes::write_blend_binding(self, description, replace)
    }

    #[func]
    pub fn get_blend_binding_snapshot(&self, id: GString) -> Dictionary {
        blendshapes::get_blend_binding_snapshot(self, id)
    }

    // --- Glue ---

    #[func]
    pub fn write_glue(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        glue::write_glue(self, description, replace)
    }

    #[func]
    pub fn get_glue_snapshot(&self, id: GString) -> Dictionary {
        glue::get_glue_snapshot(self, id)
    }

    // --- Offscreen ---

    #[func]
    pub fn write_offscreen(
        &mut self,
        description: Dictionary,
        #[opt(default = false)] replace: bool,
    ) -> Dictionary {
        offscreen::write_offscreen(self, description, replace)
    }

    #[func]
    pub fn get_offscreen_snapshot(&self, id: GString) -> Dictionary {
        offscreen::get_offscreen_snapshot(self, id)
    }

    // --- Inspection ---

    #[func]
    pub fn references_to(&self, id: GString) -> Dictionary {
        inspection::references_to(self, id)
    }

    #[func]
    pub fn erase_object(&mut self, id: GString) -> Dictionary {
        inspection::erase_object(self, id)
    }

    #[func]
    pub fn evaluate_mesh(&self, id: GString) -> Dictionary {
        inspection::evaluate_mesh(self, id)
    }

    #[func]
    pub fn get_frame(&self) -> Dictionary {
        inspection::get_frame(self)
    }

    #[func]
    pub fn get_document_state(&self) -> Dictionary {
        inspection::get_document_state(self)
    }

    #[func]
    pub fn get_document_summary(&self) -> Dictionary {
        inspection::get_document_summary(self)
    }
}
