use std::collections::HashMap;

use godot::prelude::*;
use kasane_core::types::{Mesh, Status, VertexPositionUpdate};

use crate::conversions::{
    binding_from_dict, dict_from_mesh_properties, error_dict, ids_to_packed, is_main_thread,
    mesh_properties_from_dict, packed_to_ids, packed_to_vectors, status_to_dict, vectors_to_packed,
    Array, Dictionary,
};
use crate::mesh_data::KasaneMeshData;

use super::KasaneDocumentBridge;

pub(super) fn write_mesh_internal(
    bridge: &mut KasaneDocumentBridge,
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
        if let Some(old) = bridge.session.document().get_mesh(&mesh.id) {
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
        if let Some(old) = bridge.session.document().get_mesh(&mesh.id) {
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
        bridge
            .session
            .document_mut()
            .replace_mesh_with_keyforms(mesh, binding)
    } else if replace {
        bridge.session.document_mut().replace_mesh(mesh)
    } else {
        bridge.session.document_mut().create_mesh(mesh)
    };
    bridge.apply(edit)
}

pub(super) fn create_mesh(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
) -> Dictionary {
    write_mesh_internal(bridge, description, false, None)
}

pub(super) fn replace_mesh(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
) -> Dictionary {
    write_mesh_internal(bridge, description, true, None)
}

pub(super) fn replace_mesh_with_keyforms(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
    keyforms: Dictionary,
) -> Dictionary {
    write_mesh_internal(bridge, description, true, Some(keyforms))
}

pub(super) fn get_mesh(bridge: &KasaneDocumentBridge, id: GString) -> Option<Gd<KasaneMeshData>> {
    if !is_main_thread() {
        return None;
    }
    bridge.session.document().get_mesh(&id.to_string())?;
    let mut handle = Gd::<KasaneMeshData>::default();
    handle.bind_mut().attach(
        bridge.base().instance_id().to_i64() as u64,
        bridge.generation,
        bridge.object_epoch(&id.to_string()),
        id,
    );
    Some(handle)
}

pub(super) fn replace_mesh_topology(
    bridge: &mut KasaneDocumentBridge,
    description: Dictionary,
) -> Dictionary {
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
    let edit = bridge.session.document_mut().replace_mesh_topology(
        value.mesh,
        value.binding,
        value.blend_bindings,
        value.glues,
        mapping,
    );
    bridge.apply(edit)
}

pub(super) fn get_mesh_topology_snapshot(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let doc = bridge.session.document();
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

pub(super) fn set_mesh_keyform(
    bridge: &mut KasaneDocumentBridge,
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
    if let Some(b) = bridge
        .session
        .document()
        .get_binding(&binding_id.to_string())
    {
        for old in &b.keyforms {
            if old.keys == form.keys {
                form.appearance = old.appearance;
                form.draw_order = old.draw_order;
                break;
            }
        }
    }
    let edit = bridge
        .session
        .document_mut()
        .set_mesh_keyform(&binding_id.to_string(), form);
    bridge.apply(edit)
}

pub(super) fn set_mesh_properties(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    data: Dictionary,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document requires the main thread.");
    }
    let Some(old) = bridge.session.document().get_mesh(&id.to_string()) else {
        return error_dict("MISSING_MESH", &id.to_string());
    };
    let mut m = old.clone();
    m.masks.clear();
    m.draw_order = None;
    if let Err(s) = mesh_properties_from_dict(&data, &mut m) {
        return status_to_dict(&s);
    }
    let edit = bridge.session.document_mut().replace_mesh(m);
    bridge.apply(edit)
}

pub(super) fn set_vertex_positions(
    bridge: &mut KasaneDocumentBridge,
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
    let edit = bridge.session.document_mut().set_vertex_positions(
        &mesh_id.to_string(),
        &vertices,
        &packed_to_vectors(&positions),
    );
    bridge.apply(edit)
}

pub(super) fn rename_mesh(
    bridge: &mut KasaneDocumentBridge,
    id: GString,
    name: GString,
) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let edit = bridge
        .session
        .document_mut()
        .rename_mesh(&id.to_string(), name.to_string());
    bridge.apply(edit)
}

pub(super) fn stage_vertex_positions(
    bridge: &mut KasaneDocumentBridge,
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
    let s = bridge.session.document_mut().stage_vertex_positions(update);
    status_to_dict(&s)
}

pub(super) fn commit_vertex_updates(
    bridge: &mut KasaneDocumentBridge,
    updates: Array,
    expected_revision: i64,
) -> Dictionary {
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
    let edit = bridge
        .session
        .document_mut()
        .apply_vertex_position_updates_at_revision(&batch, expected_revision as u64);
    bridge.apply(edit)
}

pub(super) fn get_mesh_snapshot(bridge: &KasaneDocumentBridge, id: GString) -> Dictionary {
    if !is_main_thread() {
        return error_dict("WRONG_THREAD", "Document bridge requires the main thread.");
    }
    let Some(mesh) = bridge.session.document().get_mesh(&id.to_string()) else {
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
    out.set("revision", bridge.session.document().revision() as i64);
    out
}
