// SPDX-License-Identifier: MIT
#include "document_bridge.hpp"
#include "model_conversion.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/os.hpp>
#include <cmath>
#include <unordered_set>

using namespace godot;

namespace kasane_gd {
void KasaneDocumentBridge::_bind_methods() {
    ADD_SIGNAL(MethodInfo("changed", PropertyInfo(Variant::DICTIONARY, "change")));
    ADD_SIGNAL(MethodInfo("preview_changed"));
    ClassDB::bind_method(D_METHOD("get_frame"), &KasaneDocumentBridge::get_frame);
    ClassDB::bind_method(D_METHOD("set_preview_values", "values"), &KasaneDocumentBridge::set_preview_values);
    ClassDB::bind_method(D_METHOD("write_part", "data", "replace"), &KasaneDocumentBridge::write_part,
                         DEFVAL(false));
    ClassDB::bind_method(D_METHOD("write_transform", "data", "replace"),
                         &KasaneDocumentBridge::write_transform, DEFVAL(false));
    ClassDB::bind_method(D_METHOD("write_scene_binding", "data", "replace"),
                         &KasaneDocumentBridge::write_scene_binding, DEFVAL(false));
    ClassDB::bind_method(D_METHOD("set_mesh_properties", "id", "data"),
                         &KasaneDocumentBridge::set_mesh_properties);
    ClassDB::bind_method(D_METHOD("create_parameter", "description"),
                         &KasaneDocumentBridge::create_parameter);
    ClassDB::bind_method(D_METHOD("write_binding", "description", "replace"),
                         &KasaneDocumentBridge::write_binding, DEFVAL(false));
    ClassDB::bind_method(D_METHOD("set_mesh_keyform", "binding_id", "keys", "positions"),
                         &KasaneDocumentBridge::set_mesh_keyform);
    ClassDB::bind_method(D_METHOD("erase_object", "id"), &KasaneDocumentBridge::erase_object);
    ClassDB::bind_method(D_METHOD("create_rotation", "id", "name", "center", "angle"),
                         &KasaneDocumentBridge::create_rotation);
    ClassDB::bind_method(D_METHOD("create_warp", "id", "name", "origin", "size", "columns", "rows"),
                         &KasaneDocumentBridge::create_warp);
    ClassDB::bind_method(D_METHOD("set_rotation", "id", "center", "angle"),
                         &KasaneDocumentBridge::set_rotation);
    ClassDB::bind_method(D_METHOD("set_warp_points", "id", "points"), &KasaneDocumentBridge::set_warp_points);
    ClassDB::bind_method(D_METHOD("set_deform_parent", "id", "parent"),
                         &KasaneDocumentBridge::set_deform_parent);
    ClassDB::bind_method(D_METHOD("set_organization_parent", "id", "parent"),
                         &KasaneDocumentBridge::set_organization_parent);
    ClassDB::bind_method(D_METHOD("get_deformer_snapshot", "id"),
                         &KasaneDocumentBridge::get_deformer_snapshot);
    ClassDB::bind_method(D_METHOD("get_deformer", "id"), &KasaneDocumentBridge::get_deformer);
    ClassDB::bind_method(D_METHOD("evaluate_mesh", "id"), &KasaneDocumentBridge::evaluate_mesh);
    ClassDB::bind_method(D_METHOD("initialize", "id", "canvas_size", "origin", "pixels_per_unit"),
                         &KasaneDocumentBridge::initialize, DEFVAL(Vector2()), DEFVAL(1.0));
    ClassDB::bind_method(D_METHOD("add_image_asset", "id", "name", "source", "width", "height"),
                         &KasaneDocumentBridge::add_image_asset);
    ClassDB::bind_method(D_METHOD("create_mesh", "description"), &KasaneDocumentBridge::create_mesh);
    ClassDB::bind_method(D_METHOD("set_vertex_positions", "mesh_id", "vertex_ids", "positions"),
                         &KasaneDocumentBridge::set_vertex_positions);
    ClassDB::bind_method(D_METHOD("rename_mesh", "id", "name"), &KasaneDocumentBridge::rename_mesh);
    ClassDB::bind_method(D_METHOD("begin_transaction"), &KasaneDocumentBridge::begin_transaction);
    ClassDB::bind_method(D_METHOD("stage_vertex_positions", "mesh_id", "vertex_ids", "positions"),
                         &KasaneDocumentBridge::stage_vertex_positions);
    ClassDB::bind_method(D_METHOD("commit_transaction"), &KasaneDocumentBridge::commit_transaction);
    ClassDB::bind_method(D_METHOD("cancel_transaction"), &KasaneDocumentBridge::cancel_transaction);
    ClassDB::bind_method(D_METHOD("get_mesh", "id"), &KasaneDocumentBridge::get_mesh);
    ClassDB::bind_method(D_METHOD("replace_mesh", "description"), &KasaneDocumentBridge::replace_mesh);
    ClassDB::bind_method(D_METHOD("capture_state"), &KasaneDocumentBridge::capture_state);
    ClassDB::bind_method(D_METHOD("restore_state", "state"), &KasaneDocumentBridge::restore_state);
    ClassDB::bind_method(D_METHOD("commit_vertex_updates", "updates", "expected_revision"),
                         &KasaneDocumentBridge::commit_vertex_updates);
    ClassDB::bind_method(D_METHOD("get_asset_snapshot", "id"), &KasaneDocumentBridge::get_asset_snapshot);
    ClassDB::bind_method(D_METHOD("get_mesh_snapshot", "id"), &KasaneDocumentBridge::get_mesh_snapshot);
    ClassDB::bind_method(D_METHOD("get_document_summary"), &KasaneDocumentBridge::get_document_summary);
}

Dictionary KasaneDocumentBridge::initialize(const String &id, Vector2 size, Vector2 origin,
                                            double pixels_per_unit) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return result(session_.document().initialize(utf8(id), {static_cast<float>(size.x),
                                                            static_cast<float>(size.y),
                                                            {float(origin.x), float(origin.y)},
                                                            float(pixels_per_unit)}));
}

Dictionary KasaneDocumentBridge::add_image_asset(const String &id, const String &name, const String &source,
                                                 int64_t width, int64_t height) {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return error("WRONG_THREAD", "Document requires the main thread.");
    if (width <= 0 || height <= 0 || uint64_t(width) > UINT32_MAX || uint64_t(height) > UINT32_MAX)
        return error("INVALID_ASSET", "Dimensions must be positive uint32 values.");
    return apply(session_.document().add_asset(
        {utf8(id), utf8(name), utf8(source), uint32_t(width), uint32_t(height), {}}));
}

Dictionary KasaneDocumentBridge::create_mesh(const Dictionary &d) {
    return write_mesh(d, false);
}

Dictionary KasaneDocumentBridge::replace_mesh(const Dictionary &d) {
    return write_mesh(d, true);
}

Dictionary KasaneDocumentBridge::write_mesh(const Dictionary &d, bool replace) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const std::pair<const char *, Variant::Type> required[] = {
        {"id", Variant::STRING},
        {"name", Variant::STRING},
        {"texture_asset_id", Variant::STRING},
        {"vertex_ids", Variant::PACKED_INT64_ARRAY},
        {"base_positions", Variant::PACKED_VECTOR2_ARRAY},
        {"uvs", Variant::PACKED_VECTOR2_ARRAY},
        {"triangles", Variant::PACKED_INT64_ARRAY}};
    for (const auto &[key, type] : required)
        if (!d.has(key) || d[key].get_type() != type)
            return error("INVALID_FIELD",
                         "Missing field or incorrect field type; see Document specification.");
    kasane::Mesh mesh;
    mesh.id = utf8(d["id"]);
    mesh.name = utf8(d["name"]);
    mesh.texture_asset_id = utf8(d["texture_asset_id"]);
    if (d.has("runtime_id")) {
        if (d["runtime_id"].get_type() != Variant::STRING)
            return error("INVALID_FIELD", "runtime_id must be a string.");
        mesh.runtime_id = utf8(d["runtime_id"]);
    } else if (replace) {
        if (const auto *old = session_.document().get_mesh(mesh.id))
            mesh.runtime_id = old->runtime_id;
    }
    if (d.has("properties")) {
        if (d["properties"].get_type() != Variant::DICTIONARY)
            return error("INVALID_FIELD", "properties must be a dictionary");
        if (auto s = mesh_properties_from_dictionary(d["properties"], mesh); !s.ok())
            return result(s);
    } else if (replace) {
        if (auto old = session_.document().get_mesh(mesh.id))
            if (auto s = mesh_properties_from_dictionary(mesh_properties_dictionary(*old), mesh); !s.ok())
                return result(s);
    }
    if (auto s = ids(d["vertex_ids"], mesh.vertex_ids); !s.ok())
        return result(s);
    mesh.base_positions = vectors(PackedVector2Array(d["base_positions"]));
    mesh.uvs = vectors(PackedVector2Array(d["uvs"]));
    std::vector<uint32_t> triangles;
    if (auto s = ids(d["triangles"], triangles); !s.ok())
        return result(s);
    if (triangles.size() % 3)
        return error("INVALID_LENGTH", "Triangle vertex IDs must be a multiple of three.");
    for (size_t i = 0; i < triangles.size(); i += 3)
        mesh.triangles.push_back({triangles[i], triangles[i + 1], triangles[i + 2]});
    return apply(replace ? session_.document().replace_mesh(std::move(mesh))
                         : session_.document().create_mesh(std::move(mesh)));
}

Dictionary KasaneDocumentBridge::set_vertex_positions(const String &mesh_id,
                                                      const PackedInt64Array &vertex_ids,
                                                      const PackedVector2Array &positions) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    std::vector<uint32_t> vertices;
    if (auto s = ids(vertex_ids, vertices); !s.ok())
        return result(s);
    return apply(session_.document().set_vertex_positions(utf8(mesh_id), vertices, vectors(positions)));
}

Dictionary KasaneDocumentBridge::rename_mesh(const String &id, const String &name) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return apply(session_.document().rename_mesh(utf8(id), utf8(name)));
}

Dictionary KasaneDocumentBridge::begin_transaction() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return result(session_.document().begin_transaction());
}

Dictionary KasaneDocumentBridge::stage_vertex_positions(const String &mesh_id,
                                                        const PackedInt64Array &vertex_ids,
                                                        const PackedVector2Array &positions) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    std::vector<uint32_t> vertices;
    if (auto status = ids(vertex_ids, vertices); !status.ok())
        return result(status);
    return result(
        session_.document().stage_vertex_positions({utf8(mesh_id), std::move(vertices), vectors(positions)}));
}

Dictionary KasaneDocumentBridge::commit_transaction() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return apply(session_.document().commit_transaction());
}

Dictionary KasaneDocumentBridge::cancel_transaction() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return result(session_.document().cancel_transaction());
}

Dictionary KasaneDocumentBridge::commit_vertex_updates(const Array &updates, int64_t expected_revision) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (expected_revision < 0)
        return error("INVALID_REVISION", "Expected revision must be nonnegative.");
    std::vector<kasane::VertexPositionUpdate> batch;
    batch.reserve(updates.size());
    for (int64_t i = 0; i < updates.size(); ++i) {
        if (updates[i].get_type() != Variant::DICTIONARY)
            return error("INVALID_FIELD", "Each update must be a Dictionary.");
        Dictionary item = updates[i];
        if (!item.has("mesh_id") || item["mesh_id"].get_type() != Variant::STRING ||
            !item.has("vertex_ids") || item["vertex_ids"].get_type() != Variant::PACKED_INT64_ARRAY ||
            !item.has("positions") || item["positions"].get_type() != Variant::PACKED_VECTOR2_ARRAY)
            return error("INVALID_FIELD", "Update requires mesh_id, vertex_ids and positions.");
        kasane::VertexPositionUpdate update;
        update.mesh_id = utf8(item["mesh_id"]);
        if (auto status = ids(item["vertex_ids"], update.vertex_ids); !status.ok())
            return result(status);
        update.positions = vectors(PackedVector2Array(item["positions"]));
        batch.push_back(std::move(update));
    }
    return apply(session_.document().apply_vertex_position_updates_at_revision(
        batch, static_cast<uint64_t>(expected_revision)));
}

Dictionary KasaneDocumentBridge::get_asset_snapshot(const String &id) const {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const auto *asset = session_.document().get_asset(utf8(id));
    if (!asset)
        return error("MISSING_ASSET", "Asset does not exist.");
    auto out = result({});
    out["id"] = string(asset->id);
    out["name"] = string(asset->name);
    out["source"] = string(asset->source);
    out["sha256"] = string(asset->sha256);
    out["width"] = static_cast<int64_t>(asset->width);
    out["height"] = static_cast<int64_t>(asset->height);
    out["revision"] = session_.document().revision();
    return out;
}

Ref<KasaneMeshData> KasaneDocumentBridge::get_mesh(const String &id) const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return {};
    if (!session_.document().get_mesh(utf8(id)))
        return {};
    Ref<KasaneMeshData> handle;
    handle.instantiate();
    handle->attach(get_instance_id(), generation_, id);
    return handle;
}

Ref<KasaneDocumentState> KasaneDocumentBridge::capture_state() const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return {};
    if (session_.document().transaction_active())
        return {};
    Ref<KasaneDocumentState> state;
    state.instantiate();
    state->document = session_.document();
    state->owner = get_instance_id();
    state->generation = generation_;
    return state;
}

Dictionary KasaneDocumentBridge::restore_state(const Ref<KasaneDocumentState> &state) {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (state.is_null() || state->owner != get_instance_id() || state->generation != generation_)
        return error("STALE_STATE", "State belongs to another document session.");
    if (session_.document().transaction_active())
        return error("TRANSACTION_ACTIVE", "Commit or cancel the transaction first.");
    session_.document().restore_from(state->document);
    preview_values_.clear();
    auto out = result({});
    out["revision"] = session_.document().revision();
    emit_signal("changed", out);
    return out;
}

Dictionary KasaneDocumentBridge::apply(const kasane::EditResult &edit) {
    auto out = result(edit.status);
    out["revision"] = edit.changes.revision;
    const auto kind = edit.changes.kind;
    out["change_kind"] = kind == kasane::ChangeKind::structure   ? "structure"
                         : kind == kasane::ChangeKind::positions ? "positions"
                         : kind == kasane::ChangeKind::metadata  ? "metadata"
                                                                 : "none";
    PackedStringArray changed;
    for (const auto &id : edit.changes.mesh_ids)
        changed.push_back(string(id));
    out["changed_meshes"] = changed;
    PackedStringArray objects, referrers;
    for (const auto &id : edit.changes.object_ids)
        objects.push_back(string(id));
    for (const auto &id : edit.referrers)
        referrers.push_back(string(id));
    out["changed_objects"] = objects;
    out["referrers"] = referrers;
    if (!edit.status.ok())
        return out;
    for (auto it = preview_values_.begin(); it != preview_values_.end();) {
        if (!session_.document().get_parameter(it->first))
            it = preview_values_.erase(it);
        else
            ++it;
    }
    if (edit.changes.kind != kasane::ChangeKind::none)
        emit_signal("changed", out);
    return out;
}

Dictionary KasaneDocumentBridge::get_mesh_snapshot(const String &id) const {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const auto *mesh = session_.document().get_mesh(utf8(id));
    if (!mesh)
        return error("MISSING_MESH", "Mesh does not exist.");
    auto out = result({});
    out["id"] = string(mesh->id);
    out["name"] = string(mesh->name);
    out["texture_asset_id"] = string(mesh->texture_asset_id);
    out["runtime_id"] = string(mesh->runtime_id);
    out["properties"] = mesh_properties_dictionary(*mesh);
    out["deform_parent"] = string(session_.document().parent_of(utf8(id)));
    out["organization_parent"] = string(session_.document().parent_of(utf8(id), true));
    out["vertex_ids"] = ids(mesh->vertex_ids);
    out["base_positions"] = vectors(mesh->base_positions);
    out["uvs"] = vectors(mesh->uvs);
    std::vector<uint32_t> triangles;
    for (const auto &triangle : mesh->triangles)
        triangles.insert(triangles.end(), triangle.begin(), triangle.end());
    out["triangles"] = ids(triangles);
    out["revision"] = session_.document().revision();
    return out;
}

Dictionary KasaneDocumentBridge::get_document_summary() const {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id()))
        return error("WRONG_THREAD", "Document bridge requires the main thread.");
    Dictionary out;
    out["schema_version"] = kasane::Document::schema_version;
    out["initialized"] = session_.document().initialized();
    out["id"] = string(session_.document().id());
    out["canvas_size"] = Vector2(session_.document().canvas().width, session_.document().canvas().height);
    out["revision"] = session_.document().revision();
    out["asset_count"] = static_cast<int64_t>(session_.document().asset_count());
    out["modified"] = session_.document().modified();
    out["transaction_active"] = session_.document().transaction_active();
    out["generation"] = generation_;
    Array meshes;
    for (const auto &id : session_.document().mesh_order()) {
        const auto *mesh = session_.document().get_mesh(id);
        Dictionary item;
        item["id"] = string(id);
        item["name"] = string(mesh->name);
        item["vertex_count"] = static_cast<int64_t>(mesh->vertex_ids.size());
        item["triangle_count"] = static_cast<int64_t>(mesh->triangles.size());
        meshes.push_back(item);
    }
    out["meshes"] = meshes;
    Array deformers;
    for (const auto &id : session_.document().deformer_order())
        deformers.push_back(get_deformer_snapshot(string(id)));
    out["deformers"] = deformers;
    Array parameters, bindings;
    for (const auto &id : session_.document().parameter_order())
        parameters.push_back(parameter_dictionary(*session_.document().get_parameter(id)));
    for (const auto &id : session_.document().binding_order())
        bindings.push_back(binding_dictionary(*session_.document().get_binding(id)));
    Array parts, transforms, scene_bindings;
    for (auto &id : session_.document().part_order())
        parts.push_back(part_dictionary(*session_.document().get_part(id)));
    for (auto &id : session_.document().transform_order())
        transforms.push_back(transform_dictionary(*session_.document().get_transform(id)));
    for (auto &id : session_.document().scene_binding_order())
        scene_bindings.push_back(scene_binding_dictionary(*session_.document().get_scene_binding(id)));
    out["parts"] = parts;
    out["transforms"] = transforms;
    out["scene_bindings"] = scene_bindings;
    out["parameters"] = parameters;
    out["bindings"] = bindings;
    out["canvas_origin"] =
        Vector2(session_.document().canvas().origin.x, session_.document().canvas().origin.y);
    out["pixels_per_unit"] = session_.document().canvas().pixels_per_unit;
    return out;
}
} // namespace kasane_gd
