use godot::prelude::*;
use kasane_core::types::{BlendMode, Status};

use crate::conversions::{
    dict_from_binding, dict_from_parameter, dict_from_part, dict_from_scene_binding,
    dict_from_transform, error_dict, ids_to_packed, is_main_thread, status_to_dict,
    vectors_to_packed, Array, Dictionary,
};

use super::KasaneDocumentBridge;

pub(super) fn references_to(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    if !bridge.session.document().contains_id(&id.to_string()) {
        return error_dict("MISSING_OBJECT", &id.to_string());
    }
    let mut out = status_to_dict(&Status::ok());
    let refs = PackedStringArray::from_iter(
        bridge
            .session
            .document()
            .references_to(&id.to_string())
            .iter()
            .map(GString::from),
    );
    out.set("referrers", &refs);
    out
}

pub(super) fn erase_object(bridge: &mut KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let edit = bridge.session.document_mut().erase_object(&id.to_string());
    if edit.status.is_ok() {
        *bridge.object_epochs.entry(id.to_string()).or_default() += 1;
    }
    bridge.apply(edit)
}

pub(super) fn evaluate_mesh(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let frame = match bridge.evaluated_frame() {
        Ok(frame) => frame,
        Err(status) => return status_to_dict(&status),
    };
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

pub(super) fn get_frame(bridge: &KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let frame = match bridge.evaluated_frame() {
        Ok(frame) => frame,
        Err(status) => return status_to_dict(&status),
    };
    let mut out = status_to_dict(&Status::ok());
    out.set("revision", bridge.session.document().revision() as i64);
    out.set("evaluated_revision", frame.source_revision as i64);
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

/// Lightweight execution/observation metadata; does not copy model keyforms.
pub(super) fn get_document_state(bridge: &KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let doc = bridge.session.document();
    let mut out = status_to_dict(&Status::ok());
    out.set("id", doc.id());
    out.set("initialized", doc.initialized());
    out.set("generation", bridge.generation as i64);
    out.set("revision", doc.revision() as i64);
    out.set("modified", doc.modified());
    out.set("transaction_active", doc.transaction_active());
    out.set("path", bridge.session.manifest().to_string_lossy().as_ref());
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

pub(super) fn get_document_summary(bridge: &KasaneDocumentBridge) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let doc = bridge.session.document();
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
        assets.push(&super::lifecycle::get_asset_snapshot(
            bridge,
            GString::from(id.as_str()),
        ));
    }
    out.set("assets", &assets);
    out.set("modified", doc.modified());
    out.set("path", bridge.session.manifest().to_string_lossy().as_ref());
    out.set("transaction_active", doc.transaction_active());
    out.set("generation", bridge.generation as i64);

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
        deformers.push(&super::deformers::get_deformer_snapshot(
            bridge,
            GString::from(id.as_str()),
        ));
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
        items.push(&crate::conversions::structured_to_dict(
            doc.get_offscreen(id).unwrap(),
        ));
    }
    out.set("offscreens", &items);

    out.set(
        "canvas_origin",
        Vector2::new(doc.canvas().origin.x, doc.canvas().origin.y),
    );
    out.set("pixels_per_unit", doc.canvas().pixels_per_unit as f64);
    out
}
