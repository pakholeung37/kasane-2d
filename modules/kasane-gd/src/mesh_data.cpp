// SPDX-License-Identifier: MIT
#include "mesh_data.hpp"
#include "document_bridge.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/core/object.hpp>
#include <godot_cpp/classes/os.hpp>
using namespace godot;

namespace kasane_gd {
void KasaneMeshData::_bind_methods() {
    ClassDB::bind_method(D_METHOD("is_valid"), &KasaneMeshData::is_valid);
    ClassDB::bind_method(D_METHOD("get_id"), &KasaneMeshData::get_id);
    ClassDB::bind_method(D_METHOD("snapshot"), &KasaneMeshData::snapshot);
    ClassDB::bind_method(D_METHOD("get_name"), &KasaneMeshData::get_name);
    ClassDB::bind_method(D_METHOD("set_name", "value"), &KasaneMeshData::set_name);
    ClassDB::bind_method(D_METHOD("get_positions"), &KasaneMeshData::get_positions);
    ClassDB::bind_method(D_METHOD("set_positions", "value"), &KasaneMeshData::set_positions);
    ClassDB::bind_method(D_METHOD("get_vertex_ids"), &KasaneMeshData::get_vertex_ids);
    ClassDB::bind_method(D_METHOD("set_vertex_positions", "ids", "positions"),
                         &KasaneMeshData::set_vertex_positions);
    ClassDB::bind_method(D_METHOD("replace_geometry", "ids", "positions", "uvs", "triangles"),
                         &KasaneMeshData::replace_geometry);
    ADD_PROPERTY(PropertyInfo(Variant::STRING, "id"), "", "get_id");
    ADD_PROPERTY(PropertyInfo(Variant::STRING, "name"), "set_name", "get_name");
    ADD_PROPERTY(PropertyInfo(Variant::PACKED_VECTOR2_ARRAY, "positions"), "set_positions", "get_positions");
    ADD_PROPERTY(PropertyInfo(Variant::PACKED_INT64_ARRAY, "vertex_ids"), "", "get_vertex_ids");
}

void KasaneMeshData::attach(uint64_t owner_id, uint64_t generation, const String &id) {
    owner_ = owner_id;
    generation_ = generation;
    id_ = id;
}

KasaneDocumentBridge *KasaneMeshData::owner() const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id())
        return nullptr;
    auto *bridge = Object::cast_to<KasaneDocumentBridge>(ObjectDB::get_instance(owner_));
    return bridge && bridge->generation() == generation_ ? bridge : nullptr;
}

bool KasaneMeshData::is_valid() const {
    auto *bridge = owner();
    return bridge && bool(bridge->get_mesh_snapshot(id_)["ok"]);
}

Dictionary KasaneMeshData::snapshot() const {
    auto *bridge = owner();
    return bridge ? bridge->get_mesh_snapshot(id_)
                  : error("STALE_HANDLE", "The owning document was closed or replaced.");
}

String KasaneMeshData::get_name() const {
    return snapshot().get("name", String());
}

void KasaneMeshData::set_name(const String &value) {
    auto *bridge = owner();
    ERR_FAIL_NULL_MSG(bridge, "The owning document was closed or replaced.");
    Dictionary edit = bridge->rename_mesh(id_, value);
    ERR_FAIL_COND_MSG(!bool(edit["ok"]), String(edit["message"]));
}

PackedVector2Array KasaneMeshData::get_positions() const {
    return snapshot().get("base_positions", PackedVector2Array());
}

PackedInt64Array KasaneMeshData::get_vertex_ids() const {
    return snapshot().get("vertex_ids", PackedInt64Array());
}

void KasaneMeshData::set_positions(const PackedVector2Array &value) {
    Dictionary edit = set_vertex_positions(get_vertex_ids(), value);
    ERR_FAIL_COND_MSG(!bool(edit["ok"]), String(edit["message"]));
}

Dictionary KasaneMeshData::set_vertex_positions(const PackedInt64Array &vertices,
                                                const PackedVector2Array &positions) {
    auto *bridge = owner();
    return bridge ? bridge->set_vertex_positions(id_, vertices, positions)
                  : error("STALE_HANDLE", "The owning document was closed or replaced.");
}

Dictionary KasaneMeshData::replace_geometry(const PackedInt64Array &vertices,
                                            const PackedVector2Array &positions,
                                            const PackedVector2Array &uvs,
                                            const PackedInt64Array &triangles) {
    auto *bridge = owner();
    if (!bridge)
        return error("STALE_HANDLE", "The owning document was closed or replaced.");
    auto data = snapshot();
    if (!bool(data["ok"]))
        return data;
    data["vertex_ids"] = vertices;
    data["base_positions"] = positions;
    data["uvs"] = uvs;
    data["triangles"] = triangles;
    return bridge->replace_mesh(data);
}
} // namespace kasane_gd
